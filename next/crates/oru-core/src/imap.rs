//! A small IMAP client over any byte stream.
//! The caller wraps TLS. Port 143 sends STARTTLS before [`pull_session`].
//! Only the newest messages in each mailbox are fetched.

use std::io::{Read, Write};

use crate::mime::{self, ParsedMail};
use crate::{Address, Mailbox, MailboxRole, MemoryStore, Message, Thread};

const MAX_LITERAL: usize = 2_000_000;
const FETCH_TAIL: u32 = 40;

pub struct Synced {
    pub store: MemoryStore,
    pub names: Vec<(String, String)>,
    pub sent_mailbox: String,
}

struct Proto<'a, S> {
    stream: &'a mut S,
    pending: Vec<u8>,
    tag: u32,
}

pub fn read_greeting<S: Read + Write>(stream: &mut S) -> Result<(), String> {
    let mut proto = Proto::new(stream);
    proto.greeting()
}

pub fn begin_starttls<S: Read + Write>(stream: &mut S) -> Result<(), String> {
    let mut proto = Proto::new(stream);
    proto.greeting()?;
    proto.exchange("STARTTLS")?;
    Ok(())
}

pub fn pull_session<S: Read + Write>(stream: &mut S, user: &str, password: &str) -> Result<Synced, String> {
    let mut proto = Proto::new(stream);
    proto.login(user, password)?;
    let listed = proto.list()?;
    let mut store = MemoryStore::new();
    let mut names = Vec::new();
    let mut sent_mailbox = String::from("Sent");
    let mut used = Vec::new();
    for mailbox in listed.into_iter().take(8) {
        if mailbox.role == MailboxRole::Sent {
            sent_mailbox = mailbox.imap_name.clone();
        }
        let slug = unique_slug(&mailbox, &used);
        used.push(slug.clone());
        names.push((slug.clone(), mailbox.imap_name.clone()));
        store.insert_mailbox(Mailbox {
            id: slug.clone(),
            name: mailbox.display,
            role: mailbox.role,
        });
        let exists = proto.select(&mailbox.imap_name)?;
        if exists == 0 {
            continue;
        }
        let start = exists.saturating_sub(FETCH_TAIL - 1).max(1);
        let fetched = proto.fetch(start, exists)?;
        for thread in group_threads(&slug, fetched) {
            store.insert_thread(thread);
        }
    }
    let _ = proto.exchange("LOGOUT");
    Ok(Synced {
        store,
        names,
        sent_mailbox,
    })
}

/// Fetches up to 40 messages whose uid is below `before_uid`.
pub fn pull_before<S: Read + Write>(
    stream: &mut S,
    user: &str,
    password: &str,
    mailbox: &str,
    slug: &str,
    before_uid: u32,
) -> Result<Vec<Thread>, String> {
    if before_uid <= 1 {
        return Ok(Vec::new());
    }
    let mut proto = Proto::new(stream);
    proto.login(user, password)?;
    proto.select(mailbox)?;
    let mut older: Vec<u32> = proto.search_uids()?.into_iter().filter(|uid| *uid < before_uid).collect();
    older.sort_unstable();
    let start = older.len().saturating_sub(FETCH_TAIL as usize);
    let window = &older[start..];
    let threads = if window.is_empty() {
        Vec::new()
    } else {
        let set = window.iter().map(|uid| uid.to_string()).collect::<Vec<_>>().join(",");
        group_threads(slug, proto.uid_fetch(&set)?)
    };
    let _ = proto.exchange("LOGOUT");
    Ok(threads)
}

pub fn set_seen<S: Read + Write>(
    stream: &mut S,
    user: &str,
    password: &str,
    mailbox: &str,
    uid: &str,
) -> Result<(), String> {
    store_flag(stream, user, password, mailbox, uid, "+FLAGS.SILENT (\\Seen)")
}

pub fn set_unseen<S: Read + Write>(
    stream: &mut S,
    user: &str,
    password: &str,
    mailbox: &str,
    uid: &str,
) -> Result<(), String> {
    store_flag(stream, user, password, mailbox, uid, "-FLAGS.SILENT (\\Seen)")
}

pub fn set_deleted<S: Read + Write>(
    stream: &mut S,
    user: &str,
    password: &str,
    mailbox: &str,
    uid: &str,
) -> Result<(), String> {
    store_flag(stream, user, password, mailbox, uid, "+FLAGS.SILENT (\\Deleted)")
}

fn store_flag<S: Read + Write>(
    stream: &mut S,
    user: &str,
    password: &str,
    mailbox: &str,
    uid: &str,
    change: &str,
) -> Result<(), String> {
    if !uid.chars().all(|ch| ch.is_ascii_digit()) {
        return Err("bad uid".into());
    }
    let mut proto = Proto::new(stream);
    proto.login(user, password)?;
    proto.select(mailbox)?;
    proto.exchange(&format!("UID STORE {uid} {change}"))?;
    let _ = proto.exchange("LOGOUT");
    Ok(())
}

pub fn append_sent<S: Read + Write>(
    stream: &mut S,
    user: &str,
    password: &str,
    mailbox: &str,
    raw: &[u8],
) -> Result<(), String> {
    let mut proto = Proto::new(stream);
    proto.login(user, password)?;
    proto.append(mailbox, raw)?;
    let _ = proto.exchange("LOGOUT");
    Ok(())
}

struct Listed {
    imap_name: String,
    display: String,
    role: MailboxRole,
}

impl<'a, S: Read + Write> Proto<'a, S> {
    fn new(stream: &'a mut S) -> Self {
        Self {
            stream,
            pending: Vec::new(),
            tag: 0,
        }
    }

    fn greeting(&mut self) -> Result<(), String> {
        let chunk = self.read_chunk()?;
        let text = String::from_utf8_lossy(&chunk);
        if text.to_ascii_uppercase().contains("OK") || text.starts_with("*") {
            return Ok(());
        }
        Err(text.trim().to_string())
    }

    fn login(&mut self, user: &str, password: &str) -> Result<(), String> {
        if user.contains(['\r', '\n']) || password.contains(['\r', '\n']) {
            return Err("credentials cannot contain a newline".into());
        }
        self.exchange(&format!("LOGIN {} {}", quote_imap(user), quote_imap(password)))?;
        Ok(())
    }

    fn list(&mut self) -> Result<Vec<Listed>, String> {
        let (lines, _) = self.exchange("LIST \"\" \"*\"")?;
        let mut found = Vec::new();
        for line in lines {
            let text = String::from_utf8_lossy(&line);
            let Some((flags, name)) = parse_list(text.trim()) else {
                continue;
            };
            if flags.iter().any(|flag| flag.eq_ignore_ascii_case("\\Noselect") || flag.eq_ignore_ascii_case("\\All"))
            {
                continue;
            }
            let name = decode_utf7(&name);
            if name.is_empty() {
                continue;
            }
            let role = role_of(&name, &flags);
            let display = if role == MailboxRole::Inbox { "Inbox".into() } else { name.clone() };
            found.push(Listed {
                imap_name: name,
                display,
                role,
            });
        }
        found.sort_by_key(|mailbox| role_rank(mailbox.role));
        if !found.iter().any(|mailbox| mailbox.role == MailboxRole::Inbox) {
            found.insert(
                0,
                Listed {
                    imap_name: "INBOX".into(),
                    display: "Inbox".into(),
                    role: MailboxRole::Inbox,
                },
            );
        }
        Ok(found)
    }

    fn select(&mut self, mailbox: &str) -> Result<u32, String> {
        let (lines, _) = self.exchange(&format!("SELECT {}", quote_imap(mailbox)))?;
        Ok(exists_count(&lines))
    }

    fn fetch(&mut self, start: u32, end: u32) -> Result<Vec<Fetched>, String> {
        self.fetch_command(&format!("FETCH {start}:{end} (UID FLAGS BODY.PEEK[])"))
    }

    fn uid_fetch(&mut self, set: &str) -> Result<Vec<Fetched>, String> {
        self.fetch_command(&format!("UID FETCH {set} (UID FLAGS BODY.PEEK[])"))
    }

    fn fetch_command(&mut self, command: &str) -> Result<Vec<Fetched>, String> {
        let (lines, _) = self.exchange(command)?;
        let mut messages = Vec::new();
        for line in lines {
            if let Some(fetched) = parse_fetch(&line) {
                messages.push(fetched);
            }
        }
        Ok(messages)
    }

    fn search_uids(&mut self) -> Result<Vec<u32>, String> {
        let (lines, _) = self.exchange("UID SEARCH ALL")?;
        let mut uids = Vec::new();
        for line in lines {
            let text = String::from_utf8_lossy(&line);
            let Some(rest) = text.trim().strip_prefix('*') else {
                continue;
            };
            let rest = rest.trim();
            if !rest.to_ascii_uppercase().starts_with("SEARCH") {
                continue;
            }
            let list = rest.split_once(' ').map(|(_, tail)| tail).unwrap_or("");
            for token in list.split_whitespace() {
                if let Ok(uid) = token.parse() {
                    uids.push(uid);
                }
            }
        }
        Ok(uids)
    }

    fn append(&mut self, mailbox: &str, raw: &[u8]) -> Result<(), String> {
        self.tag += 1;
        let tag = format!("a{:03}", self.tag);
        let command = format!("{tag} APPEND {} (\\Seen) {{{}}}\r\n", quote_imap(mailbox), raw.len());
        self.stream.write_all(command.as_bytes()).map_err(|error| error.to_string())?;
        self.stream.flush().map_err(|error| error.to_string())?;
        loop {
            let chunk = self.read_chunk()?;
            let text = String::from_utf8_lossy(&chunk);
            let trimmed = text.trim();
            if trimmed.starts_with('+') {
                break;
            }
            if trimmed.starts_with(&tag) {
                return Err(trimmed.to_string());
            }
        }
        self.stream.write_all(raw).map_err(|error| error.to_string())?;
        self.stream.write_all(b"\r\n").map_err(|error| error.to_string())?;
        self.stream.flush().map_err(|error| error.to_string())?;
        loop {
            let chunk = self.read_chunk()?;
            let text = String::from_utf8_lossy(&chunk);
            let trimmed = text.trim();
            if let Some(rest) = trimmed.strip_prefix(&tag) {
                let rest = rest.trim();
                if rest.starts_with("OK") {
                    return Ok(());
                }
                return Err(rest.to_string());
            }
        }
    }

    fn exchange(&mut self, command: &str) -> Result<(Vec<Vec<u8>>, String), String> {
        self.tag += 1;
        let tag = format!("a{:03}", self.tag);
        let line = format!("{tag} {command}\r\n");
        self.stream.write_all(line.as_bytes()).map_err(|error| error.to_string())?;
        self.stream.flush().map_err(|error| error.to_string())?;
        let mut untagged = Vec::new();
        loop {
            let chunk = self.read_chunk()?;
            let text = String::from_utf8_lossy(&chunk);
            let trimmed = text.trim();
            if let Some(rest) = trimmed.strip_prefix(&tag) {
                let rest = rest.trim();
                if rest.starts_with("OK") {
                    return Ok((untagged, rest.to_string()));
                }
                return Err(if rest.is_empty() { "command failed".into() } else { rest.to_string() });
            }
            untagged.push(chunk);
        }
    }

    fn read_chunk(&mut self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        loop {
            let line = self.read_until(b'\n')?;
            if let Some(size) = literal_len(&line) {
                if size > MAX_LITERAL {
                    return Err("message is too large".into());
                }
                out.extend_from_slice(&line);
                out.extend(self.read_exact(size)?);
                continue;
            }
            out.extend_from_slice(&line);
            return Ok(out);
        }
    }

    fn read_until(&mut self, byte: u8) -> Result<Vec<u8>, String> {
        loop {
            if let Some(pos) = self.pending.iter().position(|item| *item == byte) {
                return Ok(self.pending.drain(..=pos).collect());
            }
            let mut tmp = [0u8; 2048];
            let read = self.stream.read(&mut tmp).map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("connection closed".into());
            }
            self.pending.extend_from_slice(&tmp[..read]);
        }
    }

    fn read_exact(&mut self, size: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            if !self.pending.is_empty() {
                let take = (size - out.len()).min(self.pending.len());
                out.extend(self.pending.drain(..take));
                continue;
            }
            let mut tmp = [0u8; 2048];
            let read = self.stream.read(&mut tmp).map_err(|error| error.to_string())?;
            if read == 0 {
                return Err("connection closed".into());
            }
            self.pending.extend_from_slice(&tmp[..read]);
        }
        Ok(out)
    }
}

fn literal_len(line: &[u8]) -> Option<usize> {
    let mut end = line.len();
    if end >= 2 && line[end - 2] == b'\r' && line[end - 1] == b'\n' {
        end -= 2;
    } else if end >= 1 && line[end - 1] == b'\n' {
        end -= 1;
    } else {
        return None;
    }
    if end == 0 || line[end - 1] != b'}' {
        return None;
    }
    let mut start = end - 1;
    while start > 0 && line[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start == end - 1 || start == 0 || line[start - 1] != b'{' {
        return None;
    }
    let open = start - 1;
    if in_quotes(line, open) {
        return None;
    }
    std::str::from_utf8(&line[start..end - 1]).ok()?.parse().ok()
}

fn in_quotes(line: &[u8], at: usize) -> bool {
    let mut quoted = false;
    let mut i = 0;
    while i < at && i < line.len() {
        if line[i] == b'\\' && quoted {
            i += 2;
            continue;
        }
        if line[i] == b'"' {
            quoted = !quoted;
        }
        i += 1;
    }
    quoted
}

struct Fetched {
    uid: String,
    unread: bool,
    mail: ParsedMail,
}

fn parse_fetch(chunk: &[u8]) -> Option<Fetched> {
    let text = String::from_utf8_lossy(chunk);
    if !text.to_ascii_uppercase().contains(" FETCH ") {
        return None;
    }
    let (marker, size) = first_literal(chunk)?;
    let raw = chunk.get(marker..marker + size)?.to_vec();
    let prefix = String::from_utf8_lossy(&chunk[..marker]);
    let uid = uid_of(&prefix).or_else(|| sequence_of(&text))?;
    let flags = prefix.to_ascii_uppercase();
    if flags.contains("\\DELETED") {
        return None;
    }
    let unread = !flags.contains("\\SEEN");
    Some(Fetched {
        uid,
        unread,
        mail: mime::parse(&raw),
    })
}

fn first_literal(chunk: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < chunk.len() {
        if chunk[i] == b'{' {
            let start = i + 1;
            let mut end = start;
            while end < chunk.len() && chunk[end].is_ascii_digit() {
                end += 1;
            }
            if end > start && end < chunk.len() && chunk[end] == b'}' {
                let size: usize = std::str::from_utf8(&chunk[start..end]).ok()?.parse().ok()?;
                let mut body = end + 1;
                if body + 1 < chunk.len() && chunk[body] == b'\r' && chunk[body + 1] == b'\n' {
                    body += 2;
                } else if body < chunk.len() && chunk[body] == b'\n' {
                    body += 1;
                } else {
                    i += 1;
                    continue;
                }
                return Some((body, size));
            }
        }
        i += 1;
    }
    None
}

fn uid_of(prefix: &str) -> Option<String> {
    let upper = prefix.to_ascii_uppercase();
    let at = upper.find("UID ")?;
    let rest = prefix[at + 4..].trim_start();
    let digits: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if digits.is_empty() { None } else { Some(digits) }
}

fn sequence_of(text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix('*')?.trim();
    let number: String = rest.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    if number.is_empty() { None } else { Some(number) }
}

fn exists_count(lines: &[Vec<u8>]) -> u32 {
    let mut count = 0;
    for line in lines {
        let text = String::from_utf8_lossy(line);
        let trimmed = text.trim();
        let Some(rest) = trimmed.strip_prefix('*') else {
            continue;
        };
        let rest = rest.trim();
        let Some((number, tail)) = rest.split_once(' ') else {
            continue;
        };
        if tail.eq_ignore_ascii_case("EXISTS") {
            if let Ok(value) = number.parse() {
                count = value;
            }
        }
    }
    count
}

fn parse_list(line: &str) -> Option<(Vec<String>, String)> {
    let rest = line.strip_prefix("* LIST ")?;
    let (flags, rest) = paren_list(rest)?;
    let rest = rest.trim_start();
    let (_, rest) = imap_string(rest)?;
    let (name, _) = imap_string(rest.trim_start())?;
    Some((flags, name))
}

fn paren_list(input: &str) -> Option<(Vec<String>, &str)> {
    let input = input.trim_start();
    let rest = input.strip_prefix('(')?;
    let end = rest.find(')')?;
    let flags = rest[..end].split_whitespace().map(|flag| flag.to_string()).collect();
    Some((flags, &rest[end + 1..]))
}

fn imap_string(input: &str) -> Option<(String, &str)> {
    let input = input.trim_start();
    if let Some(rest) = input.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
                continue;
            }
            if ch == '"' {
                return Some((out, chars.as_str()));
            }
            out.push(ch);
        }
        return None;
    }
    if input.to_ascii_uppercase().starts_with("NIL") {
        return Some((String::new(), &input[3..]));
    }
    let end = input.find(|ch: char| ch.is_whitespace()).unwrap_or(input.len());
    if end == 0 {
        return None;
    }
    Some((input[..end].to_string(), &input[end..]))
}

fn role_of(name: &str, flags: &[String]) -> MailboxRole {
    let has = |flag: &str| flags.iter().any(|item| item.eq_ignore_ascii_case(flag));
    if name.eq_ignore_ascii_case("INBOX") {
        return MailboxRole::Inbox;
    }
    if has("\\Sent") {
        return MailboxRole::Sent;
    }
    if has("\\Drafts") {
        return MailboxRole::Drafts;
    }
    if has("\\Archive") {
        return MailboxRole::Archive;
    }
    let lower = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    match lower.as_str() {
        "sent" | "sent items" | "sent mail" => MailboxRole::Sent,
        "drafts" | "draft" => MailboxRole::Drafts,
        "archive" | "archives" => MailboxRole::Archive,
        _ => MailboxRole::Custom,
    }
}

fn role_rank(role: MailboxRole) -> u8 {
    match role {
        MailboxRole::Inbox => 0,
        MailboxRole::Sent => 1,
        MailboxRole::Drafts => 2,
        MailboxRole::Archive => 3,
        MailboxRole::Custom => 4,
    }
}

fn unique_slug(mailbox: &Listed, used: &[String]) -> String {
    let base = match mailbox.role {
        MailboxRole::Inbox => "inbox".into(),
        MailboxRole::Sent => "sent".into(),
        MailboxRole::Drafts => "drafts".into(),
        MailboxRole::Archive => "archive".into(),
        MailboxRole::Custom => {
            let mut slug = String::new();
            for ch in mailbox.display.chars() {
                if ch.is_ascii_alphanumeric() {
                    slug.push(ch.to_ascii_lowercase());
                } else if !slug.ends_with('-') {
                    slug.push('-');
                }
            }
            let slug = slug.trim_matches('-').to_string();
            if slug.is_empty() { "box".into() } else { slug }
        }
    };
    if !used.iter().any(|item| item == &base) {
        return base;
    }
    let mut n = 2;
    loop {
        let next = format!("{base}-{n}");
        if !used.iter().any(|item| item == &next) {
            return next;
        }
        n += 1;
    }
}

fn group_threads(slug: &str, fetched: Vec<Fetched>) -> Vec<Thread> {
    let mut threads: Vec<Thread> = Vec::new();
    let mut by_id: Vec<(String, usize)> = Vec::new();
    for item in fetched {
        let parent = item.mail.references.first().cloned();
        if let Some(parent) = parent {
            if let Some((_, index)) = by_id.iter().find(|(id, _)| id == &parent) {
                let index = *index;
                remember(&mut by_id, &item.mail.message_id, index);
                threads[index].messages.push(message_from(slug, &item));
                continue;
            }
        }
        let index = threads.len();
        remember(&mut by_id, &item.mail.message_id, index);
        threads.push(Thread {
            id: format!("{slug}:{}", item.uid),
            mailbox_id: slug.into(),
            subject: item.mail.subject.clone(),
            messages: vec![message_from(slug, &item)],
        });
    }
    threads
}

fn remember(by_id: &mut Vec<(String, usize)>, message_id: &str, index: usize) {
    let id = message_id.trim().trim_matches(|ch| ch == '<' || ch == '>').to_string();
    if !id.is_empty() {
        by_id.push((id, index));
    }
}

fn message_from(slug: &str, item: &Fetched) -> Message {
    Message {
        id: format!("{slug}:{}", item.uid),
        message_id: item.mail.message_id.clone(),
        from: item.mail.from.clone(),
        to: if item.mail.to.is_empty() {
            vec![Address::new("", "")]
        } else {
            item.mail.to.clone()
        },
        subject: item.mail.subject.clone(),
        preview: item.mail.preview.clone(),
        body: item.mail.body.clone(),
        unix_secs: item.mail.unix_secs,
        unread: item.unread,
    }
}

fn quote_imap(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        if ch == '\\' || ch == '"' {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

pub fn decode_utf7(value: &str) -> String {
    let mut out = String::new();
    let mut rest = value;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start + 1..];
        if let Some(stripped) = rest.strip_prefix('-') {
            out.push('&');
            rest = stripped;
            continue;
        }
        let Some(end) = rest.find('-') else {
            out.push('&');
            out.push_str(rest);
            return out;
        };
        let encoded = &rest[..end];
        rest = &rest[end + 1..];
        let mut b64 = encoded.replace(',', "/");
        while b64.len() % 4 != 0 {
            b64.push('=');
        }
        if let Some(bytes) = crate::base64::decode(&b64) {
            let mut i = 0;
            while i + 1 < bytes.len() {
                let code = u16::from_be_bytes([bytes[i], bytes[i + 1]]);
                if let Some(ch) = char::from_u32(code as u32) {
                    out.push(ch);
                }
                i += 2;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn raw_message(from_name: &str, from_email: &str, to: &str, subject: &str, body: &str, in_reply_to: &str) -> Vec<u8> {
    let from = if from_name.is_empty() || from_name == from_email {
        format!("<{from_email}>")
    } else if from_name.is_ascii() {
        format!("{from_name} <{from_email}>")
    } else {
        format!("=?UTF-8?B?{}?= <{from_email}>", crate::base64::encode(from_name.as_bytes()))
    };
    let mut subject_header = subject.replace(['\r', '\n'], " ");
    if !subject_header.is_ascii() {
        subject_header = format!("=?UTF-8?B?{}?=", crate::base64::encode(subject_header.as_bytes()));
    }
    let date = chrono_now();
    let reply = reply_headers(in_reply_to);
    format!(
        "From: {from}\r\nTo: <{to}>\r\nSubject: {subject_header}\r\nDate: {date}\r\nMessage-ID: <{id}>\r\n{reply}MIME-Version: 1.0\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\n{body}\r\n",
        id = format!("oru.{}@oru", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)),
    )
    .into_bytes()
}

fn reply_headers(in_reply_to: &str) -> String {
    let id = in_reply_to.trim().trim_matches(|ch| ch == '<' || ch == '>');
    if id.is_empty() || id.contains(['\r', '\n']) {
        return String::new();
    }
    format!("In-Reply-To: <{id}>\r\nReferences: <{id}>\r\n")
}

fn chrono_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0);
    format_rfc5322(secs)
}

fn format_rfc5322(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400) as u32;
    let hour = tod / 3600;
    let minute = (tod % 3600) / 60;
    let second = tod % 60;
    let (year, month, day) = civil_from_days(days);
    let week = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][days.rem_euclid(7) as usize];
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!(
        "{week}, {day:02} {} {year} {hour:02}:{minute:02}:{second:02} +0000",
        months[(month - 1) as usize]
    )
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as i64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { y + 1 } else { y };
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MailStore;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn utf7_decodes_an_accent() {
        assert_eq!(decode_utf7("caf&AOk-"), "caf\u{00e9}");
        assert_eq!(decode_utf7("a&-b"), "a&b");
    }

    #[test]
    fn pull_reads_literals_and_leaves_unseen_mail_unread() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let raw = b"From: Ada Marin <ada@studio.test>\r\nTo: You <you@oru.test>\r\nSubject: Studio notes\r\nDate: Thu, 01 Jan 1970 00:00:00 +0000\r\nMessage-ID: <m1@studio.test>\r\n\r\nThe rail stays dark.\r\n";
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).ok();
            script(&mut stream, raw);
        });
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_nodelay(true).ok();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        read_greeting(&mut client).unwrap();
        let synced = pull_session(&mut client, "ada@studio.test", "secret").unwrap();
        let threads = synced.store.threads("inbox");
        assert_eq!(threads.len(), 1);
        assert!(threads[0].unread());
        assert_eq!(threads[0].subject, "Studio notes");
        assert_eq!(threads[0].messages[0].message_id, "m1@studio.test");
        assert!(threads[0].messages.iter().all(|message| message.subject != "Gone"));
        let replied = String::from_utf8(raw_message(
            "Ada",
            "ada@studio.test",
            "you@oru.test",
            "Re: Studio",
            "Hello",
            "m1@studio.test",
        ))
        .unwrap();
        assert!(replied.contains("In-Reply-To: <m1@studio.test>\r\n"));
        assert!(replied.contains("References: <m1@studio.test>\r\n"));
        let fresh = String::from_utf8(raw_message("Ada", "ada@studio.test", "you@oru.test", "Studio", "Hello", "")).unwrap();
        assert!(!fresh.contains("In-Reply-To"));
        assert_eq!(synced.sent_mailbox, "Sent");
        assert!(synced.names.iter().any(|(slug, _)| slug == "inbox"));
        assert!(format_rfc5322(0).contains("01 Jan 1970"));
    }

    #[test]
    fn older_fetches_the_uids_below_the_watermark() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let raw = b"From: Ada <ada@studio.test>\r\nSubject: Older note\r\nDate: Thu, 01 Jan 1970 00:00:00 +0000\r\nMessage-ID: <old@studio.test>\r\n\r\nBefore the rest.\r\n";
        let (tx, rx) = std::sync::mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).ok();
            write(&mut stream, b"* OK test\r\n");
            loop {
                let line = read_line(&mut stream);
                let tag = line.split_whitespace().next().unwrap_or("a").to_string();
                let upper = line.to_ascii_uppercase();
                if upper.contains(" LOGIN ") {
                    write(&mut stream, format!("{tag} OK\r\n").as_bytes());
                } else if upper.contains(" SELECT ") {
                    write(&mut stream, b"* 3 EXISTS\r\n");
                    write(&mut stream, format!("{tag} OK\r\n").as_bytes());
                } else if upper.contains(" SEARCH ") {
                    write(&mut stream, b"* SEARCH 2 7 9\r\n");
                    write(&mut stream, format!("{tag} OK\r\n").as_bytes());
                } else if upper.contains(" FETCH ") {
                    let _ = tx.send(line);
                    write(&mut stream, format!("* 1 FETCH (UID 2 FLAGS () BODY[] {{{}}}\r\n", raw.len()).as_bytes());
                    stream.write_all(raw).unwrap();
                    write(&mut stream, b")\r\n");
                    write(&mut stream, format!("{tag} OK\r\n").as_bytes());
                } else if upper.contains(" LOGOUT ") {
                    write(&mut stream, format!("{tag} OK\r\n").as_bytes());
                    break;
                } else {
                    write(&mut stream, format!("{tag} BAD\r\n").as_bytes());
                }
            }
        });
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_nodelay(true).ok();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        read_greeting(&mut client).unwrap();
        let threads = pull_before(&mut client, "ada@studio.test", "secret", "INBOX", "inbox", 7).unwrap();
        let fetched = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(fetched.to_ascii_uppercase().contains("UID FETCH 2"), "{fetched}");
        assert!(!fetched.contains('7') && !fetched.contains('9'), "{fetched}");
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].id, "inbox:2");
        assert_eq!(threads[0].subject, "Older note");
    }

    fn script(stream: &mut std::net::TcpStream, raw: &[u8]) {
        write(stream, b"* OK test\r\n");
        loop {
            let line = read_line(stream);
            let tag = line.split_whitespace().next().unwrap_or("a").to_string();
            let upper = line.to_ascii_uppercase();
            if upper.contains(" LOGIN ") {
                write(stream, format!("{tag} OK\r\n").as_bytes());
            } else if upper.contains(" LIST ") {
                write(stream, b"* LIST (\\HasNoChildren) \"/\" \"INBOX\"\r\n");
                write(stream, b"* LIST (\\Sent) \"/\" \"Sent\"\r\n");
                write(stream, format!("{tag} OK\r\n").as_bytes());
            } else if upper.contains(" SELECT ") {
                if upper.contains("INBOX") {
                    write(stream, b"* 2 EXISTS\r\n");
                } else {
                    write(stream, b"* 0 EXISTS\r\n");
                }
                write(stream, format!("{tag} OK\r\n").as_bytes());
            } else if upper.contains(" FETCH ") {
                let gone = b"From: Ada <ada@studio.test>\r\nSubject: Gone\r\nMessage-ID: <gone@studio.test>\r\n\r\nDeleted.\r\n";
                write(stream, format!("* 1 FETCH (UID 6 FLAGS (\\Deleted) BODY[] {{{}}}\r\n", gone.len()).as_bytes());
                stream.write_all(gone).unwrap();
                write(stream, b")\r\n");
                write(stream, format!("* 2 FETCH (UID 7 FLAGS () BODY[] {{{}}}\r\n", raw.len()).as_bytes());
                stream.write_all(raw).unwrap();
                write(stream, b")\r\n");
                write(stream, format!("{tag} OK\r\n").as_bytes());
            } else if upper.contains(" LOGOUT ") {
                write(stream, format!("{tag} OK\r\n").as_bytes());
                break;
            } else {
                write(stream, format!("{tag} BAD\r\n").as_bytes());
            }
        }
    }

    fn write(stream: &mut std::net::TcpStream, bytes: &[u8]) {
        stream.write_all(bytes).unwrap();
        stream.flush().unwrap();
    }

    fn read_line(stream: &mut std::net::TcpStream) -> String {
        let mut out = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if stream.read(&mut byte).unwrap() == 0 {
                break;
            }
            out.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}
