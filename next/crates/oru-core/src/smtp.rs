//! SMTP submission over any byte stream.
//! Port 587 calls [`prepare_starttls`] on the plain socket, then [`send_mail`] on the TLS socket.
//! Port 465 calls [`send_mail`] with the greeting still waiting.

use std::io::{Read, Write};

use crate::base64;

pub struct Outgoing<'a> {
    pub user: &'a str,
    pub password: &'a str,
    pub from_email: &'a str,
    /// To, Cc, and Bcc. Each one gets its own RCPT TO. The raw message decides which headers they see.
    pub recipients: &'a [&'a str],
    pub raw: &'a [u8],
}

pub fn prepare_starttls<S: Read + Write>(stream: &mut S) -> Result<(), String> {
    let code = read_reply(stream)?;
    if code != 220 {
        return Err(format!("smtp greeting {code}"));
    }
    command(stream, "EHLO oru.local", 250)?;
    command(stream, "STARTTLS", 220)?;
    Ok(())
}

pub fn send_mail<S: Read + Write>(stream: &mut S, expect_greeting: bool, mail: &Outgoing<'_>) -> Result<(), String> {
    if mail.user.contains(['\r', '\n']) || mail.password.contains(['\r', '\n']) {
        return Err("credentials cannot contain a newline".into());
    }
    if expect_greeting {
        let code = read_reply(stream)?;
        if code != 220 {
            return Err(format!("smtp greeting {code}"));
        }
    }
    command(stream, "EHLO oru.local", 250)?;
    command(stream, "AUTH LOGIN", 334)?;
    command(stream, &base64::encode(mail.user.as_bytes()), 334)?;
    let authed = command(stream, &base64::encode(mail.password.as_bytes()), 235);
    if authed.is_err() {
        return Err("smtp login failed".into());
    }
    if mail.recipients.is_empty() {
        return Err("smtp needs a recipient".into());
    }
    command(stream, &format!("MAIL FROM:<{}>", mail.from_email), 250)?;
    for recipient in mail.recipients {
        if !recipient.contains('@') || recipient.contains(['\r', '\n', ' ']) {
            return Err("smtp recipient".into());
        }
        command(stream, &format!("RCPT TO:<{recipient}>"), 250)?;
    }
    command(stream, "DATA", 354)?;
    write_data(stream, mail.raw)?;
    let queued = read_reply(stream)?;
    if queued != 250 {
        return Err(format!("smtp data {queued}"));
    }
    let _ = command(stream, "QUIT", 221);
    Ok(())
}

fn command<S: Read + Write>(stream: &mut S, line: &str, expect: u16) -> Result<(), String> {
    stream
        .write_all(format!("{line}\r\n").as_bytes())
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    let code = read_reply(stream)?;
    if code == expect {
        Ok(())
    } else {
        Err(format!("{code}"))
    }
}

fn write_data<S: Write>(stream: &mut S, raw: &[u8]) -> Result<(), String> {
    let text = String::from_utf8_lossy(raw);
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.starts_with('.') {
            stream.write_all(b".").map_err(|error| error.to_string())?;
        }
        stream
            .write_all(format!("{line}\r\n").as_bytes())
            .map_err(|error| error.to_string())?;
    }
    stream.write_all(b".\r\n").map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())?;
    Ok(())
}

fn read_reply<S: Read>(stream: &mut S) -> Result<u16, String> {
    loop {
        let line = read_line(stream)?;
        if line.len() >= 4 && line.as_bytes()[3] == b' ' {
            return line[..3].parse().map_err(|_| format!("smtp {line}"));
        }
        if line.len() < 4 {
            return Err(format!("smtp {line}"));
        }
    }
}

fn read_line<S: Read>(stream: &mut S) -> Result<String, String> {
    let mut out = Vec::new();
    let mut byte = [0u8; 1];
    loop {
        let read = stream.read(&mut byte).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("connection closed".into());
        }
        out.push(byte[0]);
        if byte[0] == b'\n' {
            break;
        }
        if out.len() > 8192 {
            return Err("smtp line is too long".into());
        }
    }
    Ok(String::from_utf8_lossy(&out).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn submits_after_starttls_handshake() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let seen = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_nodelay(true).ok();
            write(&mut stream, b"220 ready\r\n");
            assert!(read_line_raw(&mut stream).to_ascii_uppercase().starts_with("EHLO"));
            write(&mut stream, b"250-oru\r\n250 OK\r\n");
            assert!(read_line_raw(&mut stream).to_ascii_uppercase().starts_with("STARTTLS"));
            write(&mut stream, b"220 ready\r\n");
            assert!(read_line_raw(&mut stream).to_ascii_uppercase().starts_with("EHLO"));
            write(&mut stream, b"250 OK\r\n");
            assert!(read_line_raw(&mut stream).to_ascii_uppercase().starts_with("AUTH LOGIN"));
            write(&mut stream, b"334 VXNlcm5hbWU6\r\n");
            let user = read_line_raw(&mut stream);
            write(&mut stream, b"334 UGFzc3dvcmQ6\r\n");
            let _password = read_line_raw(&mut stream);
            write(&mut stream, b"235 OK\r\n");
            assert!(read_line_raw(&mut stream).contains("MAIL FROM"));
            write(&mut stream, b"250 OK\r\n");
            assert!(read_line_raw(&mut stream).contains("RCPT TO:<ada@studio.test>"));
            write(&mut stream, b"250 OK\r\n");
            assert!(read_line_raw(&mut stream).contains("RCPT TO:<bea@studio.test>"));
            write(&mut stream, b"250 OK\r\n");
            assert_eq!(read_line_raw(&mut stream).trim(), "DATA");
            write(&mut stream, b"354 go\r\n");
            let mut data = String::new();
            loop {
                let line = read_line_raw(&mut stream);
                if line.trim() == "." {
                    break;
                }
                data.push_str(&line);
            }
            write(&mut stream, b"250 queued\r\n");
            let _ = read_line_raw(&mut stream);
            write(&mut stream, b"221 bye\r\n");
            assert!(user.contains(&base64::encode(b"ada@studio.test")));
            assert!(data.contains("Subject: Hello"));
            assert!(data.contains("Lime"));
        });
        let mut client = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        client.set_nodelay(true).ok();
        client.set_read_timeout(Some(Duration::from_secs(5))).ok();
        client.set_write_timeout(Some(Duration::from_secs(5))).ok();
        prepare_starttls(&mut client).unwrap();
        let raw = b"Subject: Hello\r\n\r\nLime\r\n";
        send_mail(
            &mut client,
            false,
            &Outgoing {
                user: "ada@studio.test",
                password: "secret",
                from_email: "ada@studio.test",
                recipients: &["ada@studio.test", "bea@studio.test"],
                raw,
            },
        )
        .unwrap();
        seen.join().unwrap();
    }

    fn write(stream: &mut std::net::TcpStream, bytes: &[u8]) {
        stream.write_all(bytes).unwrap();
    }

    fn read_line_raw(stream: &mut std::net::TcpStream) -> String {
        let mut out = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            if stream.read(&mut byte).ok().unwrap_or(0) == 0 {
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
