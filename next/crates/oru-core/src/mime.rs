//! Enough of an RFC 5322 message to fill a thread row.
//! Multipart mail prefers text/plain for the text body and keeps a sanitized HTML copy.
//! Other parts are attachments. The HTML copy is what the shell may frame.

use crate::base64;
use crate::{Address, Attachment};

const MAX_BODY: usize = 200_000;

pub struct ParsedMail {
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body: String,
    pub preview: String,
    pub unix_secs: i64,
    pub message_id: String,
    pub references: Vec<String>,
    pub html: String,
    pub attachments: Vec<Attachment>,
}

pub fn parse(raw: &[u8]) -> ParsedMail {
    let (header_bytes, body) = split_message(raw);
    let header_text = decode_latin1_or_utf8(header_bytes);
    let headers = unfold(&header_text);
    let subject = decode_words(&header(&headers, "subject"));
    let subject = if subject.trim().is_empty() {
        "(no subject)".into()
    } else {
        subject
    };
    let from = parse_address_list(&decode_words(&header(&headers, "from")))
        .into_iter()
        .next()
        .unwrap_or_else(|| Address::new("", ""));
    let to = parse_address_list(&decode_words(&header(&headers, "to")));
    let cc = parse_address_list(&decode_words(&header(&headers, "cc")));
    let bcc = parse_address_list(&decode_words(&header(&headers, "bcc")));
    let message_id = header(&headers, "message-id").trim().trim_matches(|ch| ch == '<' || ch == '>').to_string();
    let references = reference_ids(&header(&headers, "references"), &header(&headers, "in-reply-to"));
    let unix_secs = parse_date(&header(&headers, "date")).unwrap_or(0);
    let collected = collect_parts(&headers, body, 0);
    let text = collected.plain.unwrap_or_default();
    let text = if text.len() > MAX_BODY {
        let mut cut = text.chars().take(MAX_BODY).collect::<String>();
        cut.push_str("\n\n[truncated]");
        cut
    } else {
        text
    };
    let preview = preview_of(&text);
    ParsedMail {
        from,
        to,
        cc,
        bcc,
        subject,
        body: text,
        preview,
        unix_secs,
        message_id,
        references,
        html: collected.html.unwrap_or_default(),
        attachments: collected.attachments,
    }
}

struct Collected {
    plain: Option<String>,
    html: Option<String>,
    attachments: Vec<Attachment>,
}

fn collect_parts(headers: &[(String, String)], body: &[u8], depth: usize) -> Collected {
    let mut collected = Collected {
        plain: None,
        html: None,
        attachments: Vec::new(),
    };
    walk_parts(headers, body, depth, &mut collected);
    collected
}

fn walk_parts(headers: &[(String, String)], body: &[u8], depth: usize, collected: &mut Collected) {
    if depth > 6 {
        return;
    }
    let content_type = header(headers, "content-type");
    let mime = content_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    let disposition = header(headers, "content-disposition");
    let filename = parameter(&disposition, "filename").or_else(|| parameter(&content_type, "name"));
    let decoded = transfer(body, &header(headers, "content-transfer-encoding"));
    if mime.starts_with("multipart/") {
        if let Some(boundary) = parameter(&content_type, "boundary") {
            walk_multipart(&decoded, &boundary, depth, collected);
        }
        return;
    }
    if is_file(&mime, &disposition) {
        if collected.attachments.len() < 8 && decoded.len() <= MAX_BODY {
            collected.attachments.push(Attachment {
                id: String::new(),
                name: file_name(filename.as_deref().unwrap_or("file")),
                mime: if mime.is_empty() { "application/octet-stream".into() } else { mime },
                bytes: decoded,
            });
        }
        return;
    }
    let text = decode_body(&decoded, &parameter(&content_type, "charset").unwrap_or_default());
    let html_part = mime.contains("html") || text.trim_start().starts_with("<!doctype html") || text.trim_start().starts_with("<html");
    if html_part {
        if collected.html.is_none() {
            collected.html = Some(sanitize_html(&text));
        }
        if collected.plain.is_none() {
            collected.plain = Some(strip_html(&text));
        }
    } else if collected.plain.is_none() {
        collected.plain = Some(text);
    }
}

fn walk_multipart(body: &[u8], boundary: &str, depth: usize, collected: &mut Collected) {
    let marker = format!("--{boundary}");
    let text = decode_latin1_or_utf8(body);
    for part in text.split(&marker).skip(1) {
        let part = part.trim_start_matches("\r\n").trim_start_matches('\n');
        if part.starts_with("--") {
            break;
        }
        let (header_bytes, body) = split_message(part.as_bytes());
        let headers = unfold(&decode_latin1_or_utf8(header_bytes));
        walk_parts(&headers, body, depth + 1, collected);
    }
}

fn is_file(mime: &str, disposition: &str) -> bool {
    if disposition.to_ascii_lowercase().contains("attachment") {
        return true;
    }
    !(mime.is_empty() || mime.starts_with("text/plain") || mime.starts_with("text/html") || mime.starts_with("multipart/"))
}

fn file_name(value: &str) -> String {
    let name = value.rsplit(['/', '\\']).next().unwrap_or(value).trim();
    let name: String = name.chars().filter(|ch| !ch.is_control() && *ch != '"' ).take(120).collect();
    if name.is_empty() { "file".into() } else { name }
}

pub fn parse_date(value: &str) -> Option<i64> {
    let value = value.trim();
    let value = value.split_once(',').map(|(_, rest)| rest.trim()).unwrap_or(value);
    let mut parts = value.split_whitespace();
    let day: u32 = parts.next()?.parse().ok()?;
    let month = month_number(parts.next()?)?;
    let year: i32 = parts.next()?.parse().ok()?;
    let time = parts.next()?;
    let mut clock = time.split(':');
    let hour: u32 = clock.next()?.parse().ok()?;
    let minute: u32 = clock.next()?.parse().ok()?;
    let second: u32 = clock.next().unwrap_or("0").parse().ok()?;
    let zone = parts.next().unwrap_or("+0000");
    let offset = zone_offset(zone)?;
    if hour > 23 || minute > 59 || second > 60 || day == 0 || day > 31 {
        return None;
    }
    let days = days_from_civil(year, month as i32, day as i32)?;
    Some(days * 86_400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64 - offset)
}

fn days_from_civil(mut year: i32, month: i32, day: i32) -> Option<i64> {
    if !(1..=12).contains(&month) {
        return None;
    }
    year -= if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = (year - era * 400) as u64;
    let month_prime = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * month_prime as u64 + 2) / 5 + day as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era as i64 * 146097 + doe as i64 - 719468)
}

fn month_number(name: &str) -> Option<u32> {
    Some(match &name.to_ascii_lowercase()[..] {
        "jan" => 1,
        "feb" => 2,
        "mar" => 3,
        "apr" => 4,
        "may" => 5,
        "jun" => 6,
        "jul" => 7,
        "aug" => 8,
        "sep" => 9,
        "oct" => 10,
        "nov" => 11,
        "dec" => 12,
        _ => return None,
    })
}

fn zone_offset(zone: &str) -> Option<i64> {
    let zone = zone.trim();
    if matches!(zone, "GMT" | "UT" | "UTC" | "Z") {
        return Some(0);
    }
    let bytes = zone.as_bytes();
    if bytes.len() < 5 || (bytes[0] != b'+' && bytes[0] != b'-') {
        return Some(0);
    }
    let sign = if bytes[0] == b'-' { -1 } else { 1 };
    let hours: i64 = std::str::from_utf8(&bytes[1..3]).ok()?.parse().ok()?;
    let mins: i64 = std::str::from_utf8(&bytes[3..5]).ok()?.parse().ok()?;
    Some(sign * (hours * 3600 + mins * 60))
}

fn split_message(raw: &[u8]) -> (&[u8], &[u8]) {
    if let Some(pos) = find(raw, b"\r\n\r\n") {
        return (&raw[..pos], &raw[pos + 4..]);
    }
    if let Some(pos) = find(raw, b"\n\n") {
        return (&raw[..pos], &raw[pos + 2..]);
    }
    (raw, b"")
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

fn unfold(text: &str) -> Vec<(String, String)> {
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some((_, value)) = headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            continue;
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }
    headers
}

fn header(headers: &[(String, String)], name: &str) -> String {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

fn transfer(body: &[u8], encoding: &str) -> Vec<u8> {
    match encoding.trim().to_ascii_lowercase().as_str() {
        "base64" => base64::decode(&decode_latin1_or_utf8(body)).unwrap_or_else(|| body.to_vec()),
        "quoted-printable" => quoted_printable(body),
        _ => body.to_vec(),
    }
}

fn quoted_printable(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        if body[i] == b'=' {
            if i + 1 < body.len() && (body[i + 1] == b'\n' || body[i + 1] == b'\r') {
                i += 1;
                if i < body.len() && body[i] == b'\r' {
                    i += 1;
                }
                if i < body.len() && body[i] == b'\n' {
                    i += 1;
                }
                continue;
            }
            if i + 2 < body.len() {
                if let Ok(byte) = u8::from_str_radix(&decode_latin1_or_utf8(&body[i + 1..i + 3]), 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        out.push(body[i]);
        i += 1;
    }
    out
}

fn decode_body(bytes: &[u8], charset: &str) -> String {
    let charset = charset.trim().trim_matches('"').to_ascii_lowercase();
    if charset.is_empty() || charset.replace('_', "-") == "utf-8" || charset == "us-ascii" {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    bytes.iter().map(|byte| char::from(*byte)).collect()
}

fn decode_latin1_or_utf8(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn decode_words(value: &str) -> String {
    let mut out = String::new();
    let bytes = value.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'=' && i + 1 < bytes.len() && bytes[i + 1] == b'?' {
            if let Some((decoded, next)) = encoded_word(bytes, i) {
                if out.ends_with(' ') && next_word_is_encoded(bytes, next) {
                    out.pop();
                }
                out.push_str(&decoded);
                i = next;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn next_word_is_encoded(bytes: &[u8], mut index: usize) -> bool {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index + 1 < bytes.len() && bytes[index] == b'=' && bytes[index + 1] == b'?'
}

fn encoded_word(bytes: &[u8], start: usize) -> Option<(String, usize)> {
    let rest = std::str::from_utf8(&bytes[start..]).ok()?;
    let rest = rest.strip_prefix("=?")?;
    let (charset, rest) = rest.split_once('?')?;
    let (encoding, rest) = rest.split_once('?')?;
    let (data, rest) = rest.split_once("?=")?;
    let decoded = match encoding.to_ascii_lowercase().as_str() {
        "b" => base64::decode(data).map(|bytes| decode_body(&bytes, charset))?,
        "q" => decode_body(&quoted_word(data), charset),
        _ => return None,
    };
    let consumed = start + (bytes.len() - start - rest.len());
    Some((decoded, consumed))
}

fn quoted_word(data: &str) -> Vec<u8> {
    let bytes = data.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'_' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'=' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&data[i + 1..i + 3], 16) {
                out.push(byte);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn parameter(header: &str, name: &str) -> Option<String> {
    for part in header.split(';').skip(1) {
        let (key, value) = part.split_once('=')?;
        if key.trim().eq_ignore_ascii_case(name) {
            return Some(value.trim().trim_matches('"').to_string());
        }
    }
    None
}

pub fn parse_address_list(value: &str) -> Vec<Address> {
    split_addresses(value)
        .into_iter()
        .filter_map(|piece| parse_one_address(&piece))
        .collect()
}

fn split_addresses(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut angle = 0;
    for ch in value.chars() {
        match ch {
            '"' => quoted = !quoted,
            '<' if !quoted => angle += 1,
            '>' if !quoted && angle > 0 => angle -= 1,
            ',' if !quoted && angle == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

fn parse_one_address(value: &str) -> Option<Address> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(start) = value.rfind('<') {
        if let Some(end) = value[start..].find('>') {
            let email = value[start + 1..start + end].trim().to_string();
            let name = decode_words(value[..start].trim().trim_matches('"')).trim().to_string();
            if email.contains('@') {
                return Some(Address::new(if name.is_empty() { email.clone() } else { name }, email));
            }
        }
    }
    let email = value.trim_matches(|ch| ch == '"' || ch == '\'').trim();
    if email.contains('@') {
        return Some(Address::new(email, email));
    }
    None
}

fn reference_ids(references: &str, in_reply_to: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for source in [references, in_reply_to] {
        for token in source.split_whitespace() {
            let id = token.trim_matches(|ch| ch == '<' || ch == '>').trim();
            if !id.is_empty() && !ids.iter().any(|have: &String| have == id) {
                ids.push(id.to_string());
            }
        }
    }
    ids
}

const ALLOWED_TAGS: &[&str] = &[
    "a", "b", "blockquote", "br", "code", "div", "em", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "i", "img", "li", "ol",
    "p", "pre", "span", "strong", "table", "tbody", "td", "th", "thead", "tr", "u", "ul",
];

/// Markup the shell can put in a frame. Scripts, event handlers, and unsafe URLs are removed.
pub fn sanitize_html(input: &str) -> String {
    let stripped = drop_raw_elements(input);
    let chars: Vec<char> = stripped.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index] != '<' {
            push_escaped(&mut out, chars[index]);
            index += 1;
            continue;
        }
        if chars.get(index + 1) == Some(&'!') {
            index = chars[index..].iter().position(|ch| *ch == '>').map(|pos| index + pos + 1).unwrap_or(chars.len());
            continue;
        }
        let closing = chars.get(index + 1) == Some(&'/');
        let name_at = if closing { index + 2 } else { index + 1 };
        let mut end = name_at;
        while end < chars.len() && chars[end].is_ascii_alphanumeric() {
            end += 1;
        }
        let name: String = chars[name_at..end].iter().collect::<String>().to_ascii_lowercase();
        let mut cursor = end;
        let mut quote = None;
        while cursor < chars.len() {
            let ch = chars[cursor];
            if let Some(mark) = quote {
                if ch == mark {
                    quote = None;
                }
            } else if ch == '"' || ch == '\'' {
                quote = Some(ch);
            } else if ch == '>' {
                break;
            }
            cursor += 1;
        }
        let raw: String = chars[end..cursor.min(chars.len())].iter().collect();
        index = if cursor < chars.len() && chars[cursor] == '>' { cursor + 1 } else { chars.len() };
        if !ALLOWED_TAGS.contains(&name.as_str()) {
            continue;
        }
        if closing {
            out.push_str("</");
            out.push_str(&name);
            out.push('>');
            continue;
        }
        out.push('<');
        out.push_str(&name);
        write_safe_attrs(&mut out, &name, &raw);
        if matches!(name.as_str(), "br" | "hr" | "img") {
            out.push_str(" />");
        } else {
            out.push('>');
        }
    }
    out
}

fn drop_raw_elements(input: &str) -> String {
    let lower = input.to_ascii_lowercase();
    let mut skip = Vec::new();
    for tag in ["script", "style", "iframe", "object", "embed", "svg", "math", "form", "link", "meta", "base", "noscript"] {
        let open = format!("<{tag}");
        let close = format!("</{tag}");
        let mut from = 0;
        while let Some(rel) = lower[from..].find(&open) {
            let start = from + rel;
            let after = start + open.len();
            let boundary = lower.as_bytes().get(after).copied().unwrap_or(b'>');
            if !(boundary == b'>' || boundary == b'/' || boundary.is_ascii_whitespace()) {
                from = start + 1;
                continue;
            }
            if let Some(end_rel) = lower[after..].find(&close) {
                let end = after + end_rel;
                let close_end = lower[end..].find('>').map(|pos| end + pos + 1).unwrap_or(lower.len());
                skip.push((start, close_end));
                from = close_end;
            } else {
                skip.push((start, lower.len()));
                break;
            }
        }
    }
    skip.sort_by_key(|pair| pair.0);
    let mut out = String::new();
    let mut index = 0;
    for (start, end) in skip {
        if start >= index && start <= input.len() {
            out.push_str(&input[index..start]);
        }
        index = end.max(index).min(input.len());
    }
    if index < input.len() {
        out.push_str(&input[index..]);
    }
    out
}

fn write_safe_attrs(out: &mut String, tag: &str, raw: &str) {
    if tag == "a" {
        if let Some(href) = attr_value(raw, "href").and_then(|value| safe_url(&value)) {
            out.push_str(" href=\"");
            out.push_str(&escape_attr(&href));
            out.push('"');
        }
    }
    if tag == "img" {
        if let Some(src) = attr_value(raw, "src").and_then(|value| safe_image(&value)) {
            out.push_str(" src=\"");
            out.push_str(&escape_attr(&src));
            out.push('"');
        }
        if let Some(alt) = attr_value(raw, "alt") {
            out.push_str(" alt=\"");
            out.push_str(&escape_attr(&alt));
            out.push('"');
        }
    }
}

fn attr_value(raw: &str, name: &str) -> Option<String> {
    let lower = raw.to_ascii_lowercase();
    let needle = format!("{name}=");
    let start = lower.find(&needle)? + needle.len();
    let rest = &raw[start..];
    let bytes = rest.as_bytes();
    if bytes.first() == Some(&b'"') || bytes.first() == Some(&b'\'') {
        let quote = bytes[0] as char;
        let value = &rest[1..];
        let end = value.find(quote).unwrap_or(value.len());
        return Some(value[..end].to_string());
    }
    let end = rest.find(|ch: char| ch.is_whitespace() || ch == '>').unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn safe_url(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '<' | '>' | '"' | '\'')) {
        return None;
    }
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:") {
        Some(value.to_string())
    } else {
        None
    }
}

fn safe_image(value: &str) -> Option<String> {
    if safe_url(value).is_some() && value.to_ascii_lowercase().starts_with("https://") {
        return Some(value.trim().to_string());
    }
    let lower = value.trim().to_ascii_lowercase();
    let image = lower.starts_with("data:image/png")
        || lower.starts_with("data:image/jpeg")
        || lower.starts_with("data:image/gif")
        || lower.starts_with("data:image/webp");
    if image && !value.chars().any(|ch| ch.is_control() || matches!(ch, '<' | '>' | '"' | '\'')) {
        Some(value.trim().to_string())
    } else {
        None
    }
}

fn escape_attr(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        push_escaped(&mut out, ch);
    }
    out
}

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        _ => out.push(ch),
    }
}

fn strip_html(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut skip = Vec::new();
    for tag in ["script", "style"] {
        let mut from = 0;
        while let Some(start) = lower[from..].find(&format!("<{tag}")) {
            let start = from + start;
            if let Some(end) = lower[start..].find(&format!("</{tag}")) {
                let end = start + end;
                let close = lower[end..].find('>').map(|pos| end + pos + 1).unwrap_or(lower.len());
                skip.push((start, close));
                from = close;
            } else {
                break;
            }
        }
    }
    let mut kept = String::new();
    let mut index = 0;
    for (start, end) in skip {
        if start >= index && start <= html.len() {
            kept.push_str(&html[index..start]);
        }
        index = end.min(html.len());
    }
    if index < html.len() {
        kept.push_str(&html[index..]);
    }
    let mut out = String::new();
    let mut in_tag = false;
    for ch in kept.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
            out.push(' ');
        } else if !in_tag {
            out.push(ch);
        }
    }
    decode_entities(&out)
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn preview_of(body: &str) -> String {
    let line = body.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let mut preview: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        preview.push('…');
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_plain_message_and_its_date() {
        let raw = "\
From: Ada Marin <ada@studio.test>\r\n\
To: You <you@oru.test>\r\n\
Subject: =?UTF-8?B?U3R1ZGlv?= notes\r\n\
Date: Thu, 01 Jan 1970 00:00:00 +0000\r\n\
Message-ID: <m1@studio.test>\r\n\
\r\n\
The rail stays dark.\r\n";
        let mail = parse(raw.as_bytes());
        assert_eq!(mail.subject, "Studio notes");
        assert_eq!(mail.from.email, "ada@studio.test");
        assert_eq!(mail.to[0].email, "you@oru.test");
        assert_eq!(mail.unix_secs, 0);
        assert!(mail.body.contains("The rail stays dark"));
        assert_eq!(mail.message_id, "m1@studio.test");
    }

    #[test]
    fn prefers_the_plain_part_and_known_dates() {
        let raw = "\
From: desk@northwind.test\r\n\
Subject: Shipment\r\n\
Date: Sat, 01 Jan 2000 00:00:00 +0000\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/alternative; boundary=bound\r\n\
\r\n\
--bound\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
Content-Transfer-Encoding: quoted-printable\r\n\
\r\n\
Left the dock=2E\r\n\
--bound\r\n\
Content-Type: text/html\r\n\
\r\n\
<p>HTML</p>\r\n\
--bound--\r\n";
        let mail = parse(raw.as_bytes());
        assert_eq!(mail.unix_secs, 946684800);
        assert!(mail.body.contains("Left the dock."));
        assert!(!mail.body.contains("HTML"));
        assert!(mail.html.contains("<p>HTML</p>"));
        assert!(mail.attachments.is_empty());
    }

    #[test]
    fn html_drops_scripts_and_files_stay_attached() {
        let raw = "\
From: ada@studio.test\r\n\
Subject: Notes\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=bound\r\n\
\r\n\
--bound\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p onclick=\"alert(1)\">Hi <a href=\"javascript:alert(1)\">x</a> <a href=\"https://oru.test/a\">ok</a></p><script>alert(1)</script>\r\n\
--bound\r\n\
Content-Type: text/plain; name=\"invoice.txt\"\r\n\
Content-Disposition: attachment; filename=\"invoice.txt\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
VHJhY2tpbmc=\r\n\
--bound--\r\n";
        let mail = parse(raw.as_bytes());
        assert!(mail.body.contains("Hi"));
        assert!(!mail.html.to_ascii_lowercase().contains("<script"));
        assert!(!mail.html.contains("onclick"));
        assert!(!mail.html.contains("javascript:"));
        assert!(mail.html.contains("https://oru.test/a"));
        assert_eq!(mail.attachments.len(), 1);
        assert_eq!(mail.attachments[0].name, "invoice.txt");
        assert_eq!(mail.attachments[0].bytes, b"Tracking");
    }

    #[test]
    fn lists_cc_and_bcc_beside_every_to() {
        let raw = "\
From: Ada <ada@studio.test>\r\n\
To: Bea <bea@studio.test>, Cam <cam@studio.test>\r\n\
Cc: Dee <dee@studio.test>\r\n\
Bcc: Eve <eve@studio.test>\r\n\
Subject: Copies\r\n\
\r\n\
Hi\r\n";
        let mail = parse(raw.as_bytes());
        assert_eq!(mail.to.len(), 2);
        assert_eq!(mail.to[1].email, "cam@studio.test");
        assert_eq!(mail.cc[0].email, "dee@studio.test");
        assert_eq!(mail.bcc[0].email, "eve@studio.test");
    }
}
