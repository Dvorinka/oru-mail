//! One local HTTP server for the web UI, the phone layout, and the desktop window.
//! Requests run one at a time on the demo store, so the store needs no lock.
//!
//! The desktop command opens Edge or Chrome with `--app`. A private profile keeps
//! that window's process alive until the user closes it. Without the profile, Edge
//! hands `--app` to an already running browser and the spawned process exits.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use oru_core::{
    account_json, error_json, json_string, mailboxes_json, parse_account, parse_draft, parse_outbound, parse_search,
    raw_message, raw_message_attached,
    render_thread,
    thread_json, thread_list_json, threads_json, Account, Address, Attachment, MailStore, MailboxRole, MemoryStore,
    OutboundFile, Synced,
};

const FALLBACK: &str = r#"<!doctype html>
<html lang="en"><meta charset="utf-8"><title>Oru</title>
<body style="margin:0;height:100vh;display:grid;place-items:center;background:#121418;color:#e6e4de;font-family:Segoe UI,sans-serif">
<main style="max-width:36rem;padding:24px">
<h1 style="font-weight:560;letter-spacing:-.04em">Oru</h1>
<p>The mailbox API is up. Build the shell with <code>npm run build</code> in <code>next/ui</code>, then start <code>oru serve --ui ui/dist</code>.</p>
</main>
"#;

pub struct Opts {
    pub host: String,
    pub port: u16,
    pub ui: Option<PathBuf>,
}

struct Reply {
    status: u16,
    content_type: &'static str,
    /// Extra response headers, each line ending in CRLF. Empty for JSON.
    headers: String,
    body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            headers: String::new(),
            body: body.into_bytes(),
        }
    }

    fn html(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            headers: String::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    /// A download. The type stays generic so a HTML file is not rendered as a page.
    fn download(name: &str, body: Vec<u8>) -> Self {
        let clean: String = name
            .chars()
            .filter(|ch| ch.is_ascii() && !matches!(ch, '"' | '\\' | '\r' | '\n' | '/' ))
            .take(120)
            .collect();
        let clean = if clean.is_empty() { "file".to_string() } else { clean };
        Self {
            status: 200,
            content_type: "application/octet-stream",
            headers: format!("Content-Disposition: attachment; filename=\"{clean}\"\r\nX-Content-Type-Options: nosniff\r\n"),
            body,
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            headers: String::new(),
            body: Vec::new(),
        }
    }
}

pub fn serve(opts: Opts) -> std::io::Result<()> {
    let listener = bind(&opts)?;
    let ui = resolve_ui(opts.ui);
    serve_on(listener, Hub::boot(), ui);
    Ok(())
}

pub fn desktop(opts: Opts) -> std::io::Result<()> {
    let listener = bind(&opts)?;
    let port = listener.local_addr()?.port();
    let ui = resolve_ui(opts.ui);
    thread::spawn(move || serve_on(listener, Hub::boot(), ui));
    let url = format!("http://127.0.0.1:{port}/");
    wait_until_open(port);
    let Some(browser) = find_browser() else {
        eprintln!("No Edge or Chrome found. Open {url}");
        loop {
            thread::sleep(Duration::from_secs(3600));
        }
    };
    eprintln!("Opening {url}");
    let profile = env::temp_dir().join("oru-desktop");
    fs::create_dir_all(&profile)?;
    let mut child = Command::new(browser)
        .args(app_args(&url, &profile))
        .stdin(Stdio::null())
        .spawn()?;
    let _ = child.wait();
    Ok(())
}

fn bind(opts: &Opts) -> std::io::Result<TcpListener> {
    let listener = TcpListener::bind((opts.host.as_str(), opts.port))?;
    let addr = listener.local_addr()?;
    if addr.ip().is_unspecified() {
        eprintln!("Oru on http://127.0.0.1:{}/ (all interfaces)", addr.port());
    } else {
        eprintln!("Oru on http://{addr}/");
    }
    Ok(listener)
}

struct Hub {
    store: MemoryStore,
    account: Option<Account>,
    names: HashMap<String, String>,
    sent_mailbox: String,
    last_error: String,
}

impl Hub {
    fn demo() -> Self {
        Self {
            store: MemoryStore::demo(),
            account: None,
            names: HashMap::new(),
            sent_mailbox: "Sent".into(),
            last_error: String::new(),
        }
    }

    fn boot() -> Self {
        match read_account() {
            Ok(Some(account)) => signed_in(account),
            Ok(None) => Self::demo(),
            Err(error) => {
                eprintln!("{error}");
                let mut hub = Self::demo();
                hub.last_error = error;
                hub
            }
        }
    }

    fn apply_sync(&mut self, account: Account, synced: Synced) {
        self.names = synced.names.into_iter().collect();
        self.sent_mailbox = synced.sent_mailbox;
        self.store = synced.store;
        self.account = Some(account);
        self.last_error.clear();
    }
}

fn signed_in(account: Account) -> Hub {
    match crate::net::sync(&account) {
        Ok(synced) => {
            let mut hub = Hub::demo();
            hub.apply_sync(account, synced);
            hub
        }
        Err(error) => {
            eprintln!("sync: {error}");
            Hub {
                store: MemoryStore::new(),
                account: Some(account),
                names: HashMap::new(),
                sent_mailbox: "Sent".into(),
                last_error: error,
            }
        }
    }
}

fn serve_on(listener: TcpListener, mut hub: Hub, ui: Option<PathBuf>) {
    for conn in listener.incoming() {
        let Ok(mut stream) = conn else {
            continue;
        };
        let Some((method, path, body)) = read_request(&mut stream) else {
            continue;
        };
        let reply = handle(&mut hub, &method, &path, &body, ui.as_deref());
        let _ = stream.write_all(&encode(&reply));
    }
}

fn handle(hub: &mut Hub, method: &str, path: &str, body: &str, ui: Option<&Path>) -> Reply {
    if method == "OPTIONS" {
        return Reply::empty(204);
    }
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    match (method, parts.as_slice()) {
        ("GET", ["api", "status"]) => Reply::json(200, status_json(hub)),
        ("GET", ["api", "mailboxes"]) => Reply::json(200, mailboxes_json(&hub.store)),
        ("GET", ["api", "starred"]) => Reply::json(200, thread_list_json(&hub.store.starred_threads())),
        ("GET", ["api", "mailboxes", id, "threads"]) if safe_id(id) => {
            Reply::json(200, threads_json(&hub.store, id))
        }
        ("POST", ["api", "mailboxes", id, "older"]) if safe_id(id) => load_older(hub, id),
        ("POST", ["api", "mailboxes", id, "search"]) if safe_id(id) => search_box(hub, id, body),
        ("POST", ["api", "starred", "search"]) => search_box(hub, "starred", body),
        ("GET", ["api", "attachments", id]) if safe_id(id) => attachment(hub, id),
        ("GET", ["api", "threads", id]) if safe_id(id) => match thread_json(&hub.store, id) {
            Some(document) => Reply::json(200, document),
            None => Reply::json(404, error_json("no such thread")),
        },
        ("POST", ["api", "threads", id, "read"]) if safe_id(id) => mark_read(hub, id),
        ("POST", ["api", "threads", id, "unread"]) if safe_id(id) => mark_unread(hub, id),
        ("POST", ["api", "threads", id, "trash"]) if safe_id(id) => trash(hub, id),
        ("POST", ["api", "threads", id, "archive"]) if safe_id(id) => archive(hub, id),
        ("POST", ["api", "threads", id, "move", dest]) if safe_id(id) && safe_id(dest) => move_to(hub, id, dest),
        ("POST", ["api", "threads", id, "star"]) if safe_id(id) => mark_star(hub, id),
        ("POST", ["api", "sign-out"]) => sign_out(hub),
        ("POST", ["api", "sync"]) => refresh(hub),
        ("POST", ["api", "account"]) => save_and_sync(hub, body),
        ("POST", ["api", "messages"]) => send_message(hub, body),
        ("POST", ["api", "drafts"]) => save_draft(hub, body),
        ("GET", _) => serve_file(path, ui),
        _ => Reply::json(405, error_json("method not allowed")),
    }
}

fn status_json(hub: &Hub) -> String {
    let mode = if hub.account.is_some() { "account" } else { "demo" };
    let email = hub.account.as_ref().map(|account| account.email.as_str()).unwrap_or("");
    format!(
        "{{\"mode\":{},\"email\":{},\"error\":{}}}",
        json_string(mode),
        json_string(email),
        json_string(&hub.last_error)
    )
}

fn save_and_sync(hub: &mut Hub, body: &str) -> Reply {
    let account = match parse_account(body) {
        Ok(account) => account,
        Err(reason) => return Reply::json(400, error_json(reason)),
    };
    if let Err(error) = write_account(&account) {
        return Reply::json(500, error_json(&error));
    }
    match crate::net::sync(&account) {
        Ok(synced) => {
            hub.apply_sync(account, synced);
            Reply::json(200, status_json(hub))
        }
        Err(error) => {
            hub.store = MemoryStore::new();
            hub.names.clear();
            hub.account = Some(account);
            hub.last_error = error.clone();
            Reply::json(502, error_json(&error))
        }
    }
}

fn refresh(hub: &mut Hub) -> Reply {
    let Some(account) = hub.account.clone() else {
        return Reply::json(400, error_json("sign in first"));
    };
    match crate::net::sync(&account) {
        Ok(synced) => {
            hub.apply_sync(account, synced);
            Reply::json(200, status_json(hub))
        }
        Err(error) => {
            hub.last_error = error.clone();
            Reply::json(502, error_json(&error))
        }
    }
}

fn load_older(hub: &mut Hub, id: &str) -> Reply {
    let Some(account) = hub.account.clone() else {
        return Reply::json(200, threads_json(&hub.store, id));
    };
    let Some(imap_name) = hub.names.get(id).cloned() else {
        return Reply::json(404, error_json("no such mailbox"));
    };
    let Some(before) = hub.store.oldest_uid(id) else {
        return Reply::json(200, threads_json(&hub.store, id));
    };
    match crate::net::older(&account, &imap_name, id, before) {
        Ok(threads) => {
            for thread in threads {
                hub.store.insert_thread(thread);
            }
            Reply::json(200, threads_json(&hub.store, id))
        }
        Err(error) => {
            hub.last_error = error.clone();
            Reply::json(502, error_json(&error))
        }
    }
}

/// Signed-in mail runs IMAP SEARCH on that mailbox. The demo store searches
/// messages already stored, including the body, so a hit does not have to be on screen.
fn search_box(hub: &mut Hub, id: &str, body: &str) -> Reply {
    let (query, before) = match parse_search(body) {
        Ok(parsed) => parsed,
        Err(reason) => return Reply::json(400, error_json(reason)),
    };
    if id != "starred" {
        if let Some(account) = hub.account.clone() {
            if let Some(imap_name) = hub.names.get(id).cloned() {
                return match crate::net::search(&account, &imap_name, id, &query, before) {
                    Ok(threads) => {
                        for thread in &threads {
                            hub.store.insert_thread(thread.clone());
                        }
                        Reply::json(200, thread_list_json(&threads))
                    }
                    Err(error) => {
                        hub.last_error = error.clone();
                        Reply::json(502, error_json(&error))
                    }
                };
            }
        }
    }
    Reply::json(200, thread_list_json(&hub.store.search(id, &query, before)))
}

fn attachment(hub: &Hub, id: &str) -> Reply {
    match hub.store.attachment(id) {
        Some(file) => Reply::download(&file.name, file.bytes.clone()),
        None => Reply::json(404, error_json("no such file")),
    }
}

fn mark_read(hub: &mut Hub, id: &str) -> Reply {
    if !hub.store.mark_read(id) {
        return Reply::json(404, error_json("no such thread"));
    }
    flag_on_server(hub, id, ServerFlag::Seen);
    thread_reply(hub, id)
}

fn mark_unread(hub: &mut Hub, id: &str) -> Reply {
    if !hub.store.mark_unread(id) {
        return Reply::json(404, error_json("no such thread"));
    }
    flag_on_server(hub, id, ServerFlag::Unseen);
    thread_reply(hub, id)
}

fn mark_star(hub: &mut Hub, id: &str) -> Reply {
    let Some(on) = hub.store.toggle_star(id) else {
        return Reply::json(404, error_json("no such thread"));
    };
    flag_on_server(hub, id, if on { ServerFlag::Flagged } else { ServerFlag::Unflagged });
    thread_reply(hub, id)
}

fn archive(hub: &mut Hub, id: &str) -> Reply {
    if hub.store.thread(id).is_none() {
        return Reply::json(404, error_json("no such thread"));
    }
    let Some(dest) = hub.store.mailboxes().into_iter().find(|mailbox| mailbox.role == MailboxRole::Archive) else {
        return Reply::json(400, error_json("no archive mailbox"));
    };
    let dest_id = dest.id;
    move_to(hub, id, &dest_id)
}

/// Copies each message into `dest_id` when that mailbox exists on the server,
/// then stores `\Deleted` on the source. A failed copy leaves the thread where it is.
fn move_to(hub: &mut Hub, id: &str, dest_id: &str) -> Reply {
    if hub.store.thread(id).is_none() {
        return Reply::json(404, error_json("no such thread"));
    }
    if !hub.store.mailboxes().iter().any(|mailbox| mailbox.id == dest_id) {
        return Reply::json(404, error_json("no such mailbox"));
    }
    if hub.store.thread(id).is_some_and(|thread| thread.mailbox_id == dest_id) {
        return Reply::json(200, "{\"ok\":true}".into());
    }
    if let Some(account) = hub.account.clone() {
        if let Some(imap_dest) = hub.names.get(dest_id).cloned() {
            if let Some(thread) = hub.store.thread(id) {
                for message in thread.messages {
                    let Some((slug, uid)) = message.id.split_once(':') else {
                        continue;
                    };
                    if !uid.chars().all(|ch| ch.is_ascii_digit()) {
                        continue;
                    }
                    let Some(source) = hub.names.get(slug).cloned() else {
                        continue;
                    };
                    if let Err(error) = crate::net::copy_away(&account, &source, uid, &imap_dest) {
                        hub.last_error = error.clone();
                        return Reply::json(502, error_json(&error));
                    }
                }
            }
        }
    }
    if !hub.store.move_thread(id, dest_id) {
        return Reply::json(404, error_json("no such thread"));
    }
    Reply::json(200, "{\"ok\":true}".into())
}

fn trash(hub: &mut Hub, id: &str) -> Reply {
    if hub.store.thread(id).is_none() {
        return Reply::json(404, error_json("no such thread"));
    }
    flag_on_server(hub, id, ServerFlag::Deleted);
    if !hub.store.remove_thread(id) {
        return Reply::json(404, error_json("no such thread"));
    }
    Reply::json(200, "{\"ok\":true}".into())
}

fn sign_out(hub: &mut Hub) -> Reply {
    if let Err(error) = forget_account(&account_path()) {
        return Reply::json(500, error_json(&error));
    }
    *hub = Hub::demo();
    Reply::json(200, status_json(hub))
}

fn thread_reply(hub: &Hub, id: &str) -> Reply {
    match thread_json(&hub.store, id) {
        Some(document) => Reply::json(200, document),
        None => Reply::json(404, error_json("no such thread")),
    }
}

enum ServerFlag {
    Seen,
    Unseen,
    Deleted,
    Flagged,
    Unflagged,
}

/// Updates every message in the thread whose id is `mailbox:uid`.
/// Demo ids have no colon, so this stays on the local store.
fn flag_on_server(hub: &mut Hub, id: &str, flag: ServerFlag) {
    let Some(account) = hub.account.clone() else {
        return;
    };
    let Some(thread) = hub.store.thread(id) else {
        return;
    };
    for message in thread.messages {
        let Some((slug, uid)) = message.id.split_once(':') else {
            continue;
        };
        if !uid.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let Some(imap_name) = hub.names.get(slug).cloned() else {
            continue;
        };
        let result = match flag {
            ServerFlag::Seen => crate::net::mark_seen(&account, &imap_name, uid),
            ServerFlag::Unseen => crate::net::mark_unseen(&account, &imap_name, uid),
            ServerFlag::Deleted => crate::net::mark_deleted(&account, &imap_name, uid),
            ServerFlag::Flagged => crate::net::mark_flagged(&account, &imap_name, uid, true),
            ServerFlag::Unflagged => crate::net::mark_flagged(&account, &imap_name, uid, false),
        };
        if let Err(error) = result {
            hub.last_error = error;
        }
    }
}

fn drafts_name(hub: &Hub) -> String {
    hub.store
        .mailboxes()
        .into_iter()
        .find(|mailbox| mailbox.role == MailboxRole::Drafts)
        .and_then(|mailbox| hub.names.get(&mailbox.id).cloned())
        .unwrap_or_else(|| "Drafts".into())
}

fn compose_raw(name: &str, email: &str, message: &oru_core::Outbound) -> Vec<u8> {
    match &message.file {
        Some(file) => raw_message_attached(
            name,
            email,
            &message.to,
            &message.subject,
            &message.body,
            &message.in_reply_to,
            Some(&file.name),
            Some(&file.mime),
            Some(&file.bytes),
        ),
        None => raw_message(name, email, &message.to, &message.subject, &message.body, &message.in_reply_to),
    }
}

fn taken_file(file: Option<OutboundFile>) -> Option<Attachment> {
    file.map(|file| Attachment {
        id: String::new(),
        name: file.name,
        mime: file.mime,
        bytes: file.bytes,
    })
}

fn save_draft(hub: &mut Hub, body: &str) -> Reply {
    let message = match parse_draft(body) {
        Ok(message) => message,
        Err(reason) => return Reply::json(400, error_json(reason)),
    };
    if let Some(account) = hub.account.clone() {
        let raw = compose_raw(&account.name, &account.email, &message);
        let mailbox = drafts_name(hub);
        if let Err(error) = crate::net::file_sent(&account, &mailbox, &raw) {
            hub.last_error = format!("Saved here, but the server did not keep the draft: {error}");
        }
    }
    let thread = hub
        .store
        .save_draft(&message.to, &message.subject, &message.body, taken_file(message.file));
    Reply::json(201, render_thread(&thread))
}

fn send_message(hub: &mut Hub, body: &str) -> Reply {
    let message = match parse_outbound(body) {
        Ok(message) => message,
        Err(reason) => return Reply::json(400, error_json(reason)),
    };
    let Some(account) = hub.account.clone() else {
        let thread = hub.store.deliver(
            Address::new("You", "you@oru.test"),
            &message.to,
            &message.subject,
            &message.body,
            taken_file(message.file),
        );
        return Reply::json(201, render_thread(&thread));
    };
    let raw = compose_raw(&account.name, &account.email, &message);
    if let Err(error) = crate::net::submit(&account, &message.to, &raw) {
        hub.last_error = error.clone();
        return Reply::json(502, error_json(&error));
    }
    let mailbox = hub.sent_mailbox.clone();
    if let Err(error) = crate::net::file_sent(&account, &mailbox, &raw) {
        hub.last_error = format!("Sent, but the server did not keep a copy: {error}");
    }
    let thread = hub.store.deliver(
        Address::new(&account.name, &account.email),
        &message.to,
        &message.subject,
        &message.body,
        taken_file(message.file),
    );
    Reply::json(201, render_thread(&thread))
}

fn read_account() -> Result<Option<Account>, String> {
    let path = account_path();
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
    parse_account(&text).map(Some).map_err(|error| error.to_string())
}

fn forget_account(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn write_account(account: &Account) -> Result<(), String> {
    let path = account_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, account_json(account)).map_err(|error| error.to_string())
}

fn account_path() -> PathBuf {
    if let Some(path) = env::var_os("ORU_ACCOUNT") {
        return PathBuf::from(path);
    }
    if cfg!(windows) {
        if let Some(dir) = env::var_os("APPDATA") {
            return PathBuf::from(dir).join("Oru").join("account.json");
        }
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("oru").join("account.json");
    }
    PathBuf::from("account.json")
}

fn serve_file(path: &str, ui: Option<&Path>) -> Reply {
    let Some(root) = ui else {
        return if path == "/" { Reply::html(FALLBACK) } else { Reply::json(404, error_json("not found")) };
    };
    let Some(file) = file_in(root, path) else {
        return Reply::json(404, error_json("not found"));
    };
    if !file.is_file() {
        return if path == "/" {
            Reply::html(FALLBACK)
        } else {
            Reply::json(404, error_json("not found"))
        };
    }
    let Ok(root_real) = root.canonicalize() else {
        return Reply::json(404, error_json("not found"));
    };
    let Ok(file_real) = file.canonicalize() else {
        return Reply::json(404, error_json("not found"));
    };
    if !file_real.starts_with(&root_real) {
        return Reply::json(404, error_json("not found"));
    }
    match fs::read(&file_real) {
        Ok(bytes) => Reply {
            status: 200,
            content_type: content_type(&file_real),
            headers: String::new(),
            body: bytes,
        },
        Err(_) => Reply::json(404, error_json("not found")),
    }
}

fn file_in(root: &Path, url_path: &str) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    let rel = url_path.trim_start_matches('/');
    if rel.is_empty() {
        path.push("index.html");
        return Some(path);
    }
    for segment in rel.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.contains('\\') {
            return None;
        }
        path.push(segment);
    }
    if path.is_dir() {
        path.push("index.html");
    }
    Some(path)
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() < 200
        && !id.contains("..")
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.'))
}

fn read_request(stream: &mut TcpStream) -> Option<(String, String, String)> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buf = Vec::new();
    let mut tmp = [0; 2048];
    let header_end = loop {
        if buf.len() > 32_768 {
            return None;
        }
        let read = stream.read(&mut tmp).ok()?;
        if read == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..read]);
        if let Some(end) = find_subsequence(&buf, b"\r\n\r\n") {
            break end;
        }
    };
    let header = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut body = buf[header_end + 4..].to_vec();
    let length = content_length(&header).min(1_000_000);
    while body.len() < length {
        let read = stream.read(&mut tmp).ok()?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&tmp[..read]);
    }
    body.truncate(length);
    let (method, path) = parse_request_line(header.lines().next()?)?;
    let body = String::from_utf8(body).unwrap_or_default();
    Some((method, path, body))
}

fn parse_request_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let raw = parts.next()?;
    let path = raw.split('?').next()?.to_string();
    if !path.starts_with('/') {
        return None;
    }
    Some((method, path))
}

fn content_length(header: &str) -> usize {
    for line in header.split("\r\n") {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if name.eq_ignore_ascii_case("content-length") {
            return value.trim().parse().unwrap_or(0);
        }
    }
    0
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn encode(reply: &Reply) -> Vec<u8> {
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\n{extra}Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n",
        status = reply.status,
        reason = reason(reply.status),
        content_type = reply.content_type,
        len = reply.body.len(),
        extra = reply.headers,
    );
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(&reply.body);
    bytes
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    }
}

pub fn resolve_ui(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(path) = explicit {
        if path.join("index.html").is_file() {
            return Some(path);
        }
        eprintln!("{} has no index.html", path.display());
        return None;
    }
    if let Some(from_env) = env::var_os("ORU_UI") {
        let path = PathBuf::from(from_env);
        if path.join("index.html").is_file() {
            return Some(path);
        }
    }
    for candidate in ["ui/dist", "next/ui/dist", "../ui/dist"] {
        let path = PathBuf::from(candidate);
        if path.join("index.html").is_file() {
            return Some(path);
        }
    }
    None
}

fn app_args(url: &str, profile: &Path) -> Vec<String> {
    vec![
        format!("--app={url}"),
        format!("--user-data-dir={}", profile.display()),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--disable-extensions".into(),
        "--disable-sync".into(),
        "--window-size=1200,800".into(),
    ]
}

fn find_browser() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for key in ["ProgramFiles(x86)", "ProgramFiles", "LocalAppData"] {
        let Some(root) = env::var_os(key) else {
            continue;
        };
        let root = PathBuf::from(root);
        candidates.push(root.join("Microsoft\\Edge\\Application\\msedge.exe"));
        candidates.push(root.join("Google\\Chrome\\Application\\chrome.exe"));
    }
    for path in candidates {
        if path.is_file() {
            return Some(path);
        }
    }
    for name in [
        "msedge",
        "chrome",
        "microsoft-edge",
        "microsoft-edge-stable",
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
    ] {
        if let Some(found) = which(name) {
            return Some(found);
        }
    }
    None
}

fn which(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for dir in env::split_paths(&path) {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        let exe = dir.join(format!("{name}.exe"));
        if exe.is_file() {
            return Some(exe);
        }
    }
    None
}

fn wait_until_open(port: u16) {
    for _ in 0..50 {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oru_core::MailStore;
    use std::io::{Read, Write};

    #[test]
    fn routes_mail_and_rejects_parent_paths() {
        let mut hub = Hub::demo();
        let boxes = handle(&mut hub, "GET", "/api/mailboxes", "", None);
        assert_eq!(boxes.status, 200);
        assert!(std::str::from_utf8(&boxes.body).unwrap().contains("Inbox"));

        let created = handle(
            &mut hub,
            "POST",
            "/api/messages",
            r#"{"to":"ada@studio.test","subject":"Fold","body":"Lime"}"#,
            None,
        );
        assert_eq!(created.status, 201);
        let document = std::str::from_utf8(&created.body).unwrap();
        assert!(document.contains("Fold"));
        assert!(hub.store.threads("sent").iter().any(|thread| thread.subject == "Fold"));

        let missing = handle(&mut hub, "GET", "/api/threads/missing", "", None);
        assert_eq!(missing.status, 404);
        let blocked = handle(&mut hub, "GET", "/../Cargo.toml", "", None);
        assert_eq!(blocked.status, 404);

        let dir = env::temp_dir().join(format!("oru-ui-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.html"), "<h1>Oru</h1>").unwrap();
        let page = handle(&mut hub, "GET", "/", "", Some(&dir));
        assert_eq!(page.status, 200);
        assert!(std::str::from_utf8(&page.body).unwrap().contains("Oru"));
        let escape = handle(&mut hub, "GET", "/../Cargo.toml", "", Some(&dir));
        assert_eq!(escape.status, 404);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn demo_status_read_and_sync_stay_offline() {
        let mut hub = Hub::demo();
        let status = handle(&mut hub, "GET", "/api/status", "", None);
        let text = std::str::from_utf8(&status.body).unwrap();
        assert_eq!(status.status, 200);
        assert!(text.contains("\"mode\":\"demo\""));
        assert!(!text.to_ascii_lowercase().contains("password"));

        let before = handle(&mut hub, "GET", "/api/threads/t1", "", None);
        assert!(std::str::from_utf8(&before.body).unwrap().contains("\"unread\":true"));
        let read = handle(&mut hub, "POST", "/api/threads/t1/read", "", None);
        assert_eq!(read.status, 200);
        let after = std::str::from_utf8(&read.body).unwrap();
        assert!(!after.contains("\"unread\":true"));
        assert!(after.contains("\"unread\":false"));
        let starred = handle(&mut hub, "POST", "/api/threads/t2/star", "", None);
        assert_eq!(starred.status, 200);
        assert!(std::str::from_utf8(&starred.body).unwrap().contains("\"starred\":true"));
        let flagged = handle(&mut hub, "GET", "/api/starred", "", None);
        assert_eq!(flagged.status, 200);
        let flagged_text = std::str::from_utf8(&flagged.body).unwrap();
        assert!(flagged_text.contains("Shipment"));
        assert!(!flagged_text.contains("Studio notes"));
        let cleared = handle(&mut hub, "POST", "/api/threads/t2/star", "", None);
        assert_eq!(cleared.status, 200);
        assert!(std::str::from_utf8(&cleared.body).unwrap().contains("\"starred\":false"));
        assert_eq!(handle(&mut hub, "POST", "/api/threads/missing/star", "", None).status, 404);
        let found = handle(&mut hub, "POST", "/api/mailboxes/inbox/search", r#"{"q":"invoice"}"#, None);
        assert_eq!(found.status, 200);
        let found_text = std::str::from_utf8(&found.body).unwrap();
        assert!(found_text.contains("Shipment"));
        assert!(!found_text.contains("Studio notes"));
        let none = handle(&mut hub, "POST", "/api/mailboxes/inbox/search", r#"{"q":"zzzz"}"#, None);
        assert_eq!(std::str::from_utf8(&none.body).unwrap(), "[]");
        assert_eq!(handle(&mut hub, "POST", "/api/mailboxes/inbox/search", r#"{"q":""}"#, None).status, 400);
        let starred_search = handle(&mut hub, "POST", "/api/starred/search", r#"{"q":"invoice"}"#, None);
        assert_eq!(std::str::from_utf8(&starred_search.body).unwrap(), "[]");

        let sync = handle(&mut hub, "POST", "/api/sync", "", None);
        assert_eq!(sync.status, 400);
        let older = handle(&mut hub, "POST", "/api/mailboxes/inbox/older", "", None);
        assert_eq!(older.status, 200);
        let older_text = std::str::from_utf8(&older.body).unwrap();
        assert!(older_text.contains("Studio notes"));
        assert!(older_text.contains("Shipment"));
        let file = handle(&mut hub, "GET", "/api/attachments/m2:0", "", None);
        assert_eq!(file.status, 200);
        assert_eq!(file.body, b"Tracking is on the invoice.\n");
        let encoded = encode(&file);
        let head = String::from_utf8_lossy(&encoded);
        assert!(head.contains("Content-Disposition: attachment; filename=\"invoice.txt\""));
        assert!(head.contains("application/octet-stream"));
        assert_eq!(handle(&mut hub, "GET", "/api/attachments/missing:0", "", None).status, 404);
        let bad = handle(&mut hub, "POST", "/api/account", "{}", None);
        assert_eq!(bad.status, 400);
        assert!(hub.account.is_none());
    }

    #[test]
    fn trash_unread_and_sign_out_stay_on_the_demo_store() {
        let mut hub = Hub::demo();
        let unread = handle(&mut hub, "POST", "/api/threads/t2/unread", "", None);
        assert_eq!(unread.status, 200);
        assert!(std::str::from_utf8(&unread.body).unwrap().contains("\"unread\":true"));
        let trashed = handle(&mut hub, "POST", "/api/threads/t1/trash", "", None);
        assert_eq!(trashed.status, 200);
        assert!(hub.store.thread("t1").is_none());
        assert!(hub.store.thread("t2").unwrap().unread());
        let archived = handle(&mut hub, "POST", "/api/threads/t2/archive", "", None);
        assert_eq!(archived.status, 200);
        assert!(hub.store.threads("inbox").iter().all(|thread| thread.id != "t2"));
        assert!(hub.store.threads("archive").iter().any(|thread| thread.id == "t2"));

        let path = env::temp_dir().join(format!("oru-signout-{}.json", std::process::id()));
        fs::write(&path, "not-a-real-password").unwrap();
        let previous = env::var_os("ORU_ACCOUNT");
        env::set_var("ORU_ACCOUNT", &path);
        let reply = handle(&mut hub, "POST", "/api/sign-out", "", None);
        match previous {
            Some(value) => env::set_var("ORU_ACCOUNT", value),
            None => env::remove_var("ORU_ACCOUNT"),
        }
        let _ = fs::remove_file(&path);
        assert_eq!(reply.status, 200);
        assert!(std::str::from_utf8(&reply.body).unwrap().contains("\"mode\":\"demo\""));
        assert!(hub.account.is_none());
        assert!(hub.store.thread("t1").is_some());
    }

    #[test]
    fn draft_is_filed_without_sending() {
        let mut hub = Hub::demo();
        let rejected = handle(&mut hub, "POST", "/api/messages", r#"{"to":"","subject":"Hold","body":"Later"}"#, None);
        assert_eq!(rejected.status, 400);
        let saved = handle(&mut hub, "POST", "/api/drafts", r#"{"to":"","subject":"Hold","body":"Later"}"#, None);
        assert_eq!(saved.status, 201);
        assert!(std::str::from_utf8(&saved.body).unwrap().contains("Hold"));
        assert!(hub.store.threads("drafts").iter().any(|thread| thread.subject == "Hold"));
        assert!(hub.store.threads("sent").iter().all(|thread| thread.subject != "Hold"));
        let with_file = handle(
            &mut hub,
            "POST",
            "/api/messages",
            r#"{"to":"ada@studio.test","subject":"Fold","body":"Hi","fileName":"invoice.txt","fileMime":"text/html","fileData":"VHJhY2tpbmc="}"#,
            None,
        );
        assert_eq!(with_file.status, 201);
        let text = std::str::from_utf8(&with_file.body).unwrap();
        assert!(text.contains("invoice.txt"));
        let id = text.split("\"attachments\":[{\"id\":\"").nth(1).unwrap().split('"').next().unwrap();
        let download = handle(&mut hub, "GET", &format!("/api/attachments/{id}"), "", None);
        assert_eq!(download.status, 200);
        assert_eq!(download.body, b"Tracking");
        let head = String::from_utf8(encode(&download)).unwrap();
        assert!(head.contains("application/octet-stream"));
        assert!(!head.to_ascii_lowercase().contains("text/html"));
        assert_eq!(
            handle(
                &mut hub,
                "POST",
                "/api/messages",
                r#"{"to":"ada@studio.test","subject":"Fold","body":"Hi","fileData":"!!!!"}"#,
                None,
            )
            .status,
            400
        );
        assert_eq!(
            handle(&mut hub, "POST", "/api/drafts", r#"{"to":"not-an-email","subject":"Hold","body":""}"#, None).status,
            400
        );
    }

    #[test]
    fn move_sends_a_demo_thread_to_another_mailbox() {
        let mut hub = Hub::demo();
        let moved = handle(&mut hub, "POST", "/api/threads/t2/move/sent", "", None);
        assert_eq!(moved.status, 200);
        assert!(hub.store.threads("sent").iter().any(|thread| thread.id == "t2"));
        assert!(hub.store.threads("inbox").iter().all(|thread| thread.id != "t2"));
        let same = handle(&mut hub, "POST", "/api/threads/t2/move/sent", "", None);
        assert_eq!(same.status, 200);
        assert_eq!(hub.store.threads("sent").iter().filter(|thread| thread.id == "t2").count(), 1);
        assert_eq!(handle(&mut hub, "POST", "/api/threads/t2/move/nope", "", None).status, 404);
        assert_eq!(handle(&mut hub, "POST", "/api/threads/missing/move/archive", "", None).status, 404);
    }

    #[test]
    fn desktop_window_uses_a_private_profile() {
        let args = app_args("http://127.0.0.1:8787/", Path::new("/tmp/oru"));
        assert!(args.iter().any(|arg| arg == "--app=http://127.0.0.1:8787/"));
        assert!(args.iter().any(|arg| arg.starts_with("--user-data-dir=")));
    }

    #[test]
    fn serves_mailboxes_over_http() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        thread::spawn(move || serve_on(listener, Hub::demo(), None));
        let mut text = String::new();
        for _ in 0..50 {
            thread::sleep(Duration::from_millis(20));
            let mut stream = match TcpStream::connect(("127.0.0.1", port)) {
                Ok(stream) => stream,
                Err(_) => continue,
            };
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            if stream
                .write_all(b"GET /api/mailboxes HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .is_err()
            {
                continue;
            }
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            text = String::from_utf8_lossy(&buf).into_owned();
            if text.contains("Inbox") {
                break;
            }
        }
        assert!(text.contains("200"), "{text}");
        assert!(text.contains("Inbox"), "{text}");
    }
}
