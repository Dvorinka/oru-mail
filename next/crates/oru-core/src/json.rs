//! JSON the React shell reads. Field names are the contract in `next/ui`.
//! Strings are escaped here so a subject cannot break out of the document.

use crate::{MailStore, Mailbox, Message, Thread};

/// One file on a compose. The bytes are the decoded upload, capped at 200 KB.
#[derive(Debug, PartialEq, Eq)]
pub struct OutboundFile {
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// A message the shell asked to send.
#[derive(Debug, PartialEq, Eq)]
pub struct Outbound {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// Message-ID this send answers, without brackets. Empty for a new message.
    pub in_reply_to: String,
    pub file: Option<OutboundFile>,
}

pub fn mailboxes_json(store: &impl MailStore) -> String {
    let mut out = String::from("[");
    for (index, mailbox) in store.mailboxes().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_mailbox(&mut out, mailbox);
    }
    out.push(']');
    out
}

pub fn threads_json(store: &impl MailStore, mailbox_id: &str) -> String {
    thread_list_json(&store.threads(mailbox_id))
}

pub fn thread_list_json(threads: &[Thread]) -> String {
    let mut out = String::from("[");
    for (index, thread) in threads.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_summary(&mut out, thread);
    }
    out.push(']');
    out
}

pub fn thread_json(store: &impl MailStore, thread_id: &str) -> Option<String> {
    store.thread(thread_id).map(|thread| render_thread(&thread))
}

pub fn render_thread(thread: &Thread) -> String {
    let mut out = String::new();
    push_thread(&mut out, thread, true);
    out
}

/// `q` is the text to find in the open mailbox. `before` is the uid watermark for the next page.
pub fn parse_search(input: &str) -> Result<(String, Option<u32>), &'static str> {
    let pairs = parse_flat(input)?;
    let query = pairs
        .iter()
        .find(|(key, _)| key == "q")
        .map(|(_, value)| value.trim().to_string())
        .unwrap_or_default();
    if query.is_empty() || query.len() >= 200 || query.contains(['\r', '\n', '\0']) {
        return Err("bad search");
    }
    let before = match pairs.iter().find(|(key, _)| key == "before") {
        None => None,
        Some((_, value)) if value.trim().is_empty() => None,
        Some((_, value)) => Some(value.trim().parse::<u32>().map_err(|_| "bad search")?),
    };
    Ok((query, before))
}

pub fn error_json(message: &str) -> String {
    let mut out = String::from("{\"error\":");
    push_string(&mut out, message);
    out.push('}');
    out
}

/// Flat object of strings: `to`, `subject`, and `body`. Unknown keys are ignored.
pub fn parse_outbound(input: &str) -> Result<Outbound, &'static str> {
    parse_compose(input, true)
}

/// Same object as a send, but the address and subject may be empty.
pub fn parse_draft(input: &str) -> Result<Outbound, &'static str> {
    parse_compose(input, false)
}

fn parse_compose(input: &str, require_send: bool) -> Result<Outbound, &'static str> {
    let bytes = input.as_bytes();
    let mut index = 0;
    skip_ws(bytes, &mut index);
    if index >= bytes.len() || bytes[index] != b'{' {
        return Err("expected an object");
    }
    index += 1;
    let mut to = None;
    let mut subject = None;
    let mut body = None;
    let mut in_reply_to = None;
    let mut file_name = None;
    let mut file_mime = None;
    let mut file_data = None;
    loop {
        skip_ws(bytes, &mut index);
        if index < bytes.len() && bytes[index] == b'}' {
            index += 1;
            break;
        }
        let key = parse_string(bytes, &mut index)?;
        skip_ws(bytes, &mut index);
        if index >= bytes.len() || bytes[index] != b':' {
            return Err("expected a colon");
        }
        index += 1;
        skip_ws(bytes, &mut index);
        let value = parse_string(bytes, &mut index)?;
        match key.as_str() {
            "to" => to = Some(value),
            "subject" => subject = Some(value),
            "body" => body = Some(value),
            "inReplyTo" => in_reply_to = Some(value),
            "fileName" => file_name = Some(value),
            "fileMime" => file_mime = Some(value),
            "fileData" => file_data = Some(value),
            _ => {}
        }
        skip_ws(bytes, &mut index);
        if index < bytes.len() && bytes[index] == b',' {
            index += 1;
            continue;
        }
        if index < bytes.len() && bytes[index] == b'}' {
            index += 1;
            break;
        }
        return Err("expected a comma");
    }
    skip_ws(bytes, &mut index);
    if index != bytes.len() {
        return Err("trailing data");
    }
    let to = to.unwrap_or_default().trim().to_string();
    let mut subject = subject.unwrap_or_default().trim().to_string();
    if !to.is_empty() && (!to.contains('@') || to.contains(char::is_whitespace)) {
        return Err("to needs an email");
    }
    if require_send && !to.contains('@') {
        return Err("to needs an email");
    }
    if subject.is_empty() {
        if require_send {
            return Err("missing subject");
        }
        subject = "(no subject)".to_string();
    }
    let file = match file_data.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        None => None,
        Some(data) => {
            let bytes = crate::base64::decode(data).ok_or("bad attachment")?;
            if bytes.len() > 200_000 {
                return Err("attachment is too large");
            }
            Some(OutboundFile {
                name: attachment_name(file_name.as_deref().unwrap_or("")),
                mime: attachment_mime(file_mime.as_deref().unwrap_or("")),
                bytes,
            })
        }
    };
    Ok(Outbound {
        to,
        subject,
        body: body.unwrap_or_default(),
        in_reply_to: in_reply_to.unwrap_or_default().trim().trim_matches(|ch| ch == '<' || ch == '>').to_string(),
        file,
    })
}

fn attachment_name(value: &str) -> String {
    let base = value.rsplit(['/', '\\']).next().unwrap_or(value);
    let clean: String = base
        .chars()
        .filter(|ch| ch.is_ascii() && !matches!(ch, '"' | '\\' | '\r' | '\n' | '/' | ';'))
        .take(120)
        .collect();
    if clean.is_empty() { "file".into() } else { clean }
}

fn attachment_mime(value: &str) -> String {
    let mime = value.trim();
    let safe = mime.contains('/')
        && mime.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '-' | '+' | '.'));
    if safe { mime.to_string() } else { "application/octet-stream".into() }
}

fn push_mailbox(out: &mut String, mailbox: &Mailbox) {
    out.push('{');
    field(out, "id", &mailbox.id);
    out.push(',');
    field(out, "name", &mailbox.name);
    out.push(',');
    out.push_str("\"role\":\"");
    out.push_str(mailbox.role.as_str());
    out.push_str("\"}");
}

fn push_summary(out: &mut String, thread: &Thread) {
    push_thread(out, thread, false);
}

fn push_thread(out: &mut String, thread: &Thread, with_messages: bool) {
    let latest = thread.latest();
    let (from, from_email, preview, unix_secs) = match latest {
        Some(message) => (
            message.from.name.as_str(),
            message.from.email.as_str(),
            message.preview.as_str(),
            message.unix_secs,
        ),
        None => ("", "", "", 0),
    };
    let (to, to_email) = first_to(latest);
    out.push('{');
    field(out, "id", &thread.id);
    out.push(',');
    field(out, "mailboxId", &thread.mailbox_id);
    out.push(',');
    field(out, "subject", &thread.subject);
    out.push(',');
    field(out, "from", from);
    out.push(',');
    field(out, "fromEmail", from_email);
    out.push(',');
    field(out, "to", to);
    out.push(',');
    field(out, "toEmail", to_email);
    out.push(',');
    field(out, "preview", preview);
    out.push(',');
    out.push_str("\"unixSecs\":");
    out.push_str(&unix_secs.to_string());
    out.push(',');
    out.push_str("\"unread\":");
    out.push_str(if thread.unread() { "true" } else { "false" });
    out.push(',');
    out.push_str("\"starred\":");
    out.push_str(if thread.starred() { "true" } else { "false" });
    if with_messages {
        out.push(',');
        out.push_str("\"messages\":[");
        for (index, message) in thread.messages.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            push_message(out, message);
        }
        out.push(']');
    }
    out.push('}');
}

fn push_message(out: &mut String, message: &Message) {
    let (to, to_email) = match message.to.first() {
        Some(address) => (address.name.as_str(), address.email.as_str()),
        None => ("", ""),
    };
    out.push('{');
    field(out, "id", &message.id);
    out.push(',');
    field(out, "messageId", &message.message_id);
    out.push(',');
    field(out, "from", &message.from.name);
    out.push(',');
    field(out, "fromEmail", &message.from.email);
    out.push(',');
    field(out, "to", to);
    out.push(',');
    field(out, "toEmail", to_email);
    out.push(',');
    field(out, "subject", &message.subject);
    out.push(',');
    field(out, "preview", &message.preview);
    out.push(',');
    field(out, "body", &message.body);
    out.push(',');
    field(out, "html", &crate::mime::sanitize_html(&message.html));
    out.push(',');
    out.push_str("\"attachments\":[");
    for (index, attachment) in message.attachments.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push('{');
        field(out, "id", &attachment.id);
        out.push(',');
        field(out, "name", &attachment.name);
        out.push(',');
        field(out, "mime", &attachment.mime);
        out.push(',');
        out.push_str("\"size\":");
        out.push_str(&attachment.bytes.len().to_string());
        out.push('}');
    }
    out.push(']');
    out.push(',');
    out.push_str("\"unixSecs\":");
    out.push_str(&message.unix_secs.to_string());
    out.push(',');
    out.push_str("\"unread\":");
    out.push_str(if message.unread { "true" } else { "false" });
    out.push(',');
    out.push_str("\"starred\":");
    out.push_str(if message.starred { "true" } else { "false" });
    out.push('}');
}

fn first_to(message: Option<&Message>) -> (&str, &str) {
    message
        .and_then(|message| message.to.first())
        .map(|address| (address.name.as_str(), address.email.as_str()))
        .unwrap_or(("", ""))
}

fn field(out: &mut String, name: &str, value: &str) {
    out.push('"');
    out.push_str(name);
    out.push_str("\":");
    push_string(out, value);
}

pub fn json_string(value: &str) -> String {
    let mut out = String::new();
    push_string(&mut out, value);
    out
}

fn push_string(out: &mut String, value: &str) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn skip_ws(bytes: &[u8], index: &mut usize) {
    while *index < bytes.len() && matches!(bytes[*index], b' ' | b'\t' | b'\n' | b'\r') {
        *index += 1;
    }
}

fn parse_string(bytes: &[u8], index: &mut usize) -> Result<String, &'static str> {
    if *index >= bytes.len() || bytes[*index] != b'"' {
        return Err("expected a string");
    }
    *index += 1;
    let mut raw = Vec::new();
    while *index < bytes.len() {
        match bytes[*index] {
            b'"' => {
                *index += 1;
                return String::from_utf8(raw).map_err(|_| "bad utf-8");
            }
            b'\\' => {
                *index += 1;
                if *index >= bytes.len() {
                    return Err("bad escape");
                }
                let escaped = bytes[*index];
                *index += 1;
                match escaped {
                    b'"' => raw.push(b'"'),
                    b'\\' => raw.push(b'\\'),
                    b'/' => raw.push(b'/'),
                    b'n' => raw.push(b'\n'),
                    b'r' => raw.push(b'\r'),
                    b't' => raw.push(b'\t'),
                    b'u' => {
                        if *index + 4 > bytes.len() {
                            return Err("bad escape");
                        }
                        let hex = std::str::from_utf8(&bytes[*index..*index + 4]).map_err(|_| "bad escape")?;
                        let code = u32::from_str_radix(hex, 16).map_err(|_| "bad escape")?;
                        let ch = char::from_u32(code).ok_or("bad escape")?;
                        let mut buf = [0; 4];
                        raw.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        *index += 4;
                    }
                    _ => return Err("bad escape"),
                }
            }
            byte => {
                raw.push(byte);
                *index += 1;
            }
        }
    }
    Err("unterminated string")
}

/// One JSON object whose values are strings or numbers. Nested values are rejected.
pub fn parse_flat(input: &str) -> Result<Vec<(String, String)>, &'static str> {
    let bytes = input.as_bytes();
    let mut index = 0;
    skip_ws(bytes, &mut index);
    if index >= bytes.len() || bytes[index] != b'{' {
        return Err("expected an object");
    }
    index += 1;
    let mut pairs = Vec::new();
    loop {
        skip_ws(bytes, &mut index);
        if index < bytes.len() && bytes[index] == b'}' {
            index += 1;
            break;
        }
        let key = parse_string(bytes, &mut index)?;
        skip_ws(bytes, &mut index);
        if index >= bytes.len() || bytes[index] != b':' {
            return Err("expected a colon");
        }
        index += 1;
        skip_ws(bytes, &mut index);
        let value = if index < bytes.len() && bytes[index] == b'"' {
            parse_string(bytes, &mut index)?
        } else {
            parse_number(bytes, &mut index)?
        };
        pairs.push((key, value));
        skip_ws(bytes, &mut index);
        if index < bytes.len() && bytes[index] == b',' {
            index += 1;
            continue;
        }
        if index < bytes.len() && bytes[index] == b'}' {
            index += 1;
            break;
        }
        return Err("expected a comma");
    }
    skip_ws(bytes, &mut index);
    if index != bytes.len() {
        return Err("trailing data");
    }
    Ok(pairs)
}

fn parse_number(bytes: &[u8], index: &mut usize) -> Result<String, &'static str> {
    let start = *index;
    if *index < bytes.len() && (bytes[*index] == b'-' || bytes[*index] == b'+') {
        *index += 1;
    }
    let digits = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index == digits {
        return Err("expected a string");
    }
    let text = std::str::from_utf8(&bytes[start..*index]).map_err(|_| "bad utf-8")?;
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryStore;

    #[test]
    fn shell_fields_and_newest_order() {
        let store = MemoryStore::demo();
        let boxes = mailboxes_json(&store);
        assert!(boxes.contains("\"id\":\"inbox\""));
        assert!(boxes.contains("\"role\":\"sent\""));
        let threads = threads_json(&store, "inbox");
        let first = threads.find("\"id\":\"t1\"").unwrap();
        let second = threads.find("\"id\":\"t2\"").unwrap();
        assert!(first < second);
        assert!(threads.contains("\"unread\":true"));
        assert!(threads.contains("\"starred\":false"));
        assert!(!threads.contains("\"body\""));
        let one = thread_json(&store, "t1").unwrap();
        assert!(one.contains("The rail stays dark"));
        assert!(one.contains("\"messages\":["));
        assert!(one.contains("\"messageId\":\"m1@studio.test\""));
        let sent = thread_json(&store, "t3").unwrap();
        assert!(sent.contains("square mark"));
        assert!(!sent.to_ascii_lowercase().contains("<script"));
        assert!(!sent.contains("alert(1)"));
        let shipped = thread_json(&store, "t2").unwrap();
        assert!(shipped.contains("\"name\":\"invoice.txt\""));
        assert!(!shipped.contains("Tracking is on the invoice.\\n"));
    }

    #[test]
    fn quotes_and_newlines_stay_inside_strings() {
        let mut store = MemoryStore::demo();
        store.insert_thread(crate::Thread {
            id: "qx".into(),
            mailbox_id: "inbox".into(),
            subject: "Say \"hi\"".into(),
            messages: vec![crate::Message {
                id: "mq".into(),
                message_id: "mq@studio.test".into(),
                from: crate::Address::new("Ada", "ada@studio.test"),
                to: vec![],
                subject: "Say \"hi\"".into(),
                preview: "line\n2".into(),
                body: "line\n2".into(),
                html: String::new(),
                attachments: Vec::new(),
                unix_secs: 1,
                unread: false,
                starred: false,
            }],
        });
        let threads = threads_json(&store, "inbox");
        assert!(threads.contains("Say \\\"hi\\\""));
        let one = thread_json(&store, "qx").unwrap();
        assert!(one.contains("line\\n2"));
    }

    #[test]
    fn outbound_parses_escapes_and_rejects_a_bad_address() {
        let parsed = parse_outbound(r#"{"to":"ada@studio.test","subject":"Say \"hi\"","body":"line\n2","extra":"x"}"#)
            .unwrap();
        assert_eq!(parsed.subject, "Say \"hi\"");
        assert_eq!(parsed.body, "line\n2");
        assert!(parsed.file.is_none());
        assert_eq!(parsed.in_reply_to, "");
        let reply = parse_outbound(r#"{"to":"ada@studio.test","subject":"Re: Hi","body":"","inReplyTo":"<m1@studio.test>"}"#)
            .unwrap();
        assert_eq!(reply.in_reply_to, "m1@studio.test");
        assert!(parse_outbound(r#"{"to":"not-an-email","subject":"Hi","body":""}"#).is_err());
        assert!(parse_outbound(r#"{"to":"a@b.c","subject":"  ","body":""}"#).is_err());
        let draft = parse_draft(r#"{"to":"","subject":"","body":"Later"}"#).unwrap();
        assert_eq!(draft.to, "");
        assert_eq!(draft.subject, "(no subject)");
        assert!(parse_draft(r#"{"to":"not-an-email","subject":"Hi","body":""}"#).is_err());
        let letter = parse_outbound(r#"{"to":"a@b.c","subject":"\u0041","body":""}"#).unwrap();
        assert_eq!(letter.subject, "A");
        let (query, before) = parse_search(r#"{"q":" invoice ","before":"12"}"#).unwrap();
        assert_eq!(query, "invoice");
        assert_eq!(before, Some(12));
        assert!(parse_search(r#"{"q":""}"#).is_err());
        assert!(parse_search(r#"{"q":"line\nbreak"}"#).is_err());
        let with_file = parse_outbound(
            r#"{"to":"ada@studio.test","subject":"Fold","body":"Hi","fileName":"notes/invoice.txt","fileMime":"text/plain","fileData":"VHJhY2tpbmc="}"#,
        )
        .unwrap();
        let file = with_file.file.unwrap();
        assert_eq!(file.name, "invoice.txt");
        assert_eq!(file.bytes, b"Tracking");
        assert!(parse_outbound(r#"{"to":"ada@studio.test","subject":"Fold","body":"","fileData":"!!!!"}"#).is_err());
    }
}
