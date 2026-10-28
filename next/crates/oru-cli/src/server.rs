//! One local HTTP server for the web UI, the phone layout, and the desktop window.
//! Requests run one at a time on the demo store, so the store needs no lock.
//!
//! The desktop command opens Edge or Chrome with `--app`. A private profile keeps
//! that window's process alive until the user closes it. Without the profile, Edge
//! hands `--app` to an already running browser and the spawned process exits.

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use oru_core::{error_json, mailboxes_json, parse_outbound, render_thread, thread_json, threads_json, MemoryStore};

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
    body: Vec<u8>,
}

impl Reply {
    fn json(status: u16, body: String) -> Self {
        Self {
            status,
            content_type: "application/json; charset=utf-8",
            body: body.into_bytes(),
        }
    }

    fn html(body: &str) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            content_type: "text/plain; charset=utf-8",
            body: Vec::new(),
        }
    }
}

pub fn serve(opts: Opts) -> std::io::Result<()> {
    let listener = bind(&opts)?;
    let ui = resolve_ui(opts.ui);
    serve_on(listener, MemoryStore::demo(), ui);
    Ok(())
}

pub fn desktop(opts: Opts) -> std::io::Result<()> {
    let listener = bind(&opts)?;
    let port = listener.local_addr()?.port();
    let ui = resolve_ui(opts.ui);
    thread::spawn(move || serve_on(listener, MemoryStore::demo(), ui));
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

fn serve_on(listener: TcpListener, mut store: MemoryStore, ui: Option<PathBuf>) {
    for conn in listener.incoming() {
        let Ok(mut stream) = conn else {
            continue;
        };
        let Some((method, path, body)) = read_request(&mut stream) else {
            continue;
        };
        let reply = handle(&mut store, &method, &path, &body, ui.as_deref());
        let _ = stream.write_all(&encode(&reply));
    }
}

fn handle(store: &mut MemoryStore, method: &str, path: &str, body: &str, ui: Option<&Path>) -> Reply {
    if method == "OPTIONS" {
        return Reply::empty(204);
    }
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    match (method, parts.as_slice()) {
        ("GET", ["api", "mailboxes"]) => Reply::json(200, mailboxes_json(store)),
        ("GET", ["api", "mailboxes", id, "threads"]) if safe_id(id) => {
            Reply::json(200, threads_json(store, id))
        }
        ("GET", ["api", "threads", id]) if safe_id(id) => match thread_json(store, id) {
            Some(document) => Reply::json(200, document),
            None => Reply::json(404, error_json("no such thread")),
        },
        ("POST", ["api", "messages"]) => match parse_outbound(body) {
            Ok(message) => {
                let thread = store.send(&message.to, &message.subject, &message.body);
                Reply::json(201, render_thread(&thread))
            }
            Err(reason) => Reply::json(400, error_json(reason)),
        },
        ("GET", _) => serve_file(path, ui),
        _ => Reply::json(405, error_json("method not allowed")),
    }
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
    !id.is_empty() && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
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
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {len}\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nConnection: close\r\n\r\n",
        status = reply.status,
        reason = reason(reply.status),
        content_type = reply.content_type,
        len = reply.body.len(),
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
        let mut store = MemoryStore::demo();
        let boxes = handle(&mut store, "GET", "/api/mailboxes", "", None);
        assert_eq!(boxes.status, 200);
        assert!(std::str::from_utf8(&boxes.body).unwrap().contains("Inbox"));

        let created = handle(
            &mut store,
            "POST",
            "/api/messages",
            r#"{"to":"ada@studio.test","subject":"Fold","body":"Lime"}"#,
            None,
        );
        assert_eq!(created.status, 201);
        let document = std::str::from_utf8(&created.body).unwrap();
        assert!(document.contains("Fold"));
        assert!(store.threads("sent").iter().any(|thread| thread.subject == "Fold"));

        let missing = handle(&mut store, "GET", "/api/threads/missing", "", None);
        assert_eq!(missing.status, 404);
        let blocked = handle(&mut store, "GET", "/../Cargo.toml", "", None);
        assert_eq!(blocked.status, 404);

        let dir = env::temp_dir().join(format!("oru-ui-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("index.html"), "<h1>Oru</h1>").unwrap();
        let page = handle(&mut store, "GET", "/", "", Some(&dir));
        assert_eq!(page.status, 200);
        assert!(std::str::from_utf8(&page.body).unwrap().contains("Oru"));
        let escape = handle(&mut store, "GET", "/../Cargo.toml", "", Some(&dir));
        assert_eq!(escape.status, 404);
        let _ = fs::remove_dir_all(&dir);
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
        thread::spawn(move || serve_on(listener, MemoryStore::demo(), None));
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
