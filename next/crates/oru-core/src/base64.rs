//! Encode and decode standard base64. SMTP AUTH and mail bodies both need it.

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | data[i + 2] as u32;
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
        out.push(ALPHABET[(n & 63) as usize] as char);
        i += 3;
    }
    if i < data.len() {
        let left = data.len() - i;
        let mut n = (data[i] as u32) << 16;
        if left == 2 {
            n |= (data[i + 1] as u32) << 8;
        }
        out.push(ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 63) as usize] as char);
        if left == 2 {
            out.push(ALPHABET[((n >> 6) & 63) as usize] as char);
            out.push('=');
        } else {
            out.push('=');
            out.push('=');
        }
    }
    out
}

pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut nibbles = Vec::new();
    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            continue;
        }
        if ch == '=' {
            break;
        }
        nibbles.push(value(ch)?);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= nibbles.len() {
        let n = (nibbles[i] << 18) | (nibbles[i + 1] << 12) | (nibbles[i + 2] << 6) | nibbles[i + 3];
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
        i += 4;
    }
    let rest = nibbles.len() - i;
    if rest == 2 {
        let n = (nibbles[i] << 18) | (nibbles[i + 1] << 12);
        out.push((n >> 16) as u8);
    } else if rest == 3 {
        let n = (nibbles[i] << 18) | (nibbles[i + 1] << 12) | (nibbles[i + 2] << 6);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
    } else if rest == 1 {
        return None;
    }
    Some(out)
}

fn value(ch: char) -> Option<u32> {
    match ch {
        'A'..='Z' => Some((ch as u32) - ('A' as u32)),
        'a'..='z' => Some((ch as u32) - ('a' as u32) + 26),
        '0'..='9' => Some((ch as u32) - ('0' as u32) + 52),
        '+' => Some(62),
        '/' => Some(63),
        ',' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_padding() {
        assert_eq!(encode(b"hello"), "aGVsbG8=");
        assert_eq!(decode("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(decode("YQ==").unwrap(), b"a");
        assert_eq!(decode("YWI=").unwrap(), b"ab");
    }
}
