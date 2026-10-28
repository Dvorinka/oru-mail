//! JSON the React shell reads. Field names are the contract in `next/ui`.
//! Strings are escaped here so a subject cannot break out of the document.

use crate::{MailStore, Mailbox, Message, Thread};

/// A message the shell asked to send.
#[derive(Debug, PartialEq, Eq)]
pub struct Outbound {
    pub to: String,
    pub subject: String,
    pub body: String,
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
    let mut out = String::from("[");
    for (index, thread) in store.threads(mailbox_id).iter().enumerate() {
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

pub fn error_json(message: &str) -> String {
    let mut out = String::from("{\"error\":");
    push_string(&mut out, message);
    out.push('}');
    out
}

/// Flat object of strings: `to`, `subject`, and `body`. Unknown keys are ignored.
pub fn parse_outbound(input: &str) -> Result<Outbound, &'static str> {
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
    let subject = subject.unwrap_or_default().trim().to_string();
    if !to.contains('@') || to.contains(char::is_whitespace) {
        return Err("to needs an email");
    }
    if subject.is_empty() {
        return Err("missing subject");
    }
    Ok(Outbound {
        to,
        subject,
        body: body.unwrap_or_default(),
    })
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
    out.push_str("\"unixSecs\":");
    out.push_str(&message.unix_secs.to_string());
    out.push(',');
    out.push_str("\"unread\":");
    out.push_str(if message.unread { "true" } else { "false" });
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
        assert!(!threads.contains("\"body\""));
        let one = thread_json(&store, "t1").unwrap();
        assert!(one.contains("The rail stays dark"));
        assert!(one.contains("\"messages\":["));
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
                from: crate::Address::new("Ada", "ada@studio.test"),
                to: vec![],
                subject: "Say \"hi\"".into(),
                preview: "line\n2".into(),
                body: "line\n2".into(),
                unix_secs: 1,
                unread: false,
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
        assert!(parse_outbound(r#"{"to":"not-an-email","subject":"Hi","body":""}"#).is_err());
        assert!(parse_outbound(r#"{"to":"a@b.c","subject":"  ","body":""}"#).is_err());
        let letter = parse_outbound(r#"{"to":"a@b.c","subject":"\u0041","body":""}"#).unwrap();
        assert_eq!(letter.subject, "A");
    }
}
