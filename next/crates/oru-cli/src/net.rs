//! TLS around the IMAP and SMTP sessions. The protocol itself is in oru-core.

use std::net::TcpStream;
use std::time::Duration;

use native_tls::TlsConnector;
use oru_core::{
    append_sent, begin_starttls, pull_before, pull_matching, pull_session, read_greeting, send_mail, set_seen, Account,
    Outgoing,
    Synced,
    Thread,
};

pub fn sync(account: &Account) -> Result<Synced, String> {
    let mut stream = open_imap(account)?;
    pull_session(&mut stream, &account.email, &account.password)
}

pub fn older(account: &Account, mailbox: &str, slug: &str, before_uid: u32) -> Result<Vec<Thread>, String> {
    let mut stream = open_imap(account)?;
    pull_before(&mut stream, &account.email, &account.password, mailbox, slug, before_uid)
}

pub fn search(
    account: &Account,
    mailbox: &str,
    slug: &str,
    query: &str,
    before_uid: Option<u32>,
) -> Result<Vec<Thread>, String> {
    let mut stream = open_imap(account)?;
    pull_matching(&mut stream, &account.email, &account.password, mailbox, slug, query, before_uid)
}

pub fn mark_seen(account: &Account, mailbox: &str, uid: &str) -> Result<(), String> {
    let mut stream = open_imap(account)?;
    set_seen(&mut stream, &account.email, &account.password, mailbox, uid)
}

pub fn mark_unseen(account: &Account, mailbox: &str, uid: &str) -> Result<(), String> {
    let mut stream = open_imap(account)?;
    oru_core::set_unseen(&mut stream, &account.email, &account.password, mailbox, uid)
}

pub fn copy_away(account: &Account, source: &str, uid: &str, dest: &str) -> Result<(), String> {
    let mut stream = open_imap(account)?;
    oru_core::copy_away(&mut stream, &account.email, &account.password, source, uid, dest)
}

pub fn mark_deleted(account: &Account, mailbox: &str, uid: &str) -> Result<(), String> {
    let mut stream = open_imap(account)?;
    oru_core::set_deleted(&mut stream, &account.email, &account.password, mailbox, uid)
}

pub fn mark_flagged(account: &Account, mailbox: &str, uid: &str, on: bool) -> Result<(), String> {
    let mut stream = open_imap(account)?;
    oru_core::set_flagged(&mut stream, &account.email, &account.password, mailbox, uid, on)
}

pub fn submit(account: &Account, to: &str, raw: &[u8]) -> Result<(), String> {
    let connector = tls()?;
    let mail = Outgoing {
        user: &account.email,
        password: &account.password,
        from_email: &account.email,
        to,
        raw,
    };
    if account.smtp_port == 465 {
        let tcp = plain(&account.smtp_host, account.smtp_port)?;
        let mut stream = connector
            .connect(&account.smtp_host, tcp)
            .map_err(|error| error.to_string())?;
        send_mail(&mut stream, true, &mail)
    } else {
        let mut tcp = plain(&account.smtp_host, account.smtp_port)?;
        prepare(&mut tcp)?;
        let mut stream = connector
            .connect(&account.smtp_host, tcp)
            .map_err(|error| error.to_string())?;
        send_mail(&mut stream, false, &mail)
    }
}

pub fn file_sent(account: &Account, mailbox: &str, raw: &[u8]) -> Result<(), String> {
    let mut stream = open_imap(account)?;
    append_sent(&mut stream, &account.email, &account.password, mailbox, raw)
}

fn prepare(stream: &mut TcpStream) -> Result<(), String> {
    oru_core::prepare_starttls(stream)
}

fn open_imap(account: &Account) -> Result<native_tls::TlsStream<TcpStream>, String> {
    let connector = tls()?;
    if account.imap_port == 143 {
        let mut tcp = plain(&account.imap_host, account.imap_port)?;
        begin_starttls(&mut tcp)?;
        return connector
            .connect(&account.imap_host, tcp)
            .map_err(|error| error.to_string());
    }
    let tcp = plain(&account.imap_host, account.imap_port)?;
    let mut stream = connector
        .connect(&account.imap_host, tcp)
        .map_err(|error| error.to_string())?;
    read_greeting(&mut stream)?;
    Ok(stream)
}

fn tls() -> Result<TlsConnector, String> {
    TlsConnector::new().map_err(|error| error.to_string())
}

fn plain(host: &str, port: u16) -> Result<TcpStream, String> {
    let stream = TcpStream::connect((host, port)).map_err(|error| format!("{host}:{port} {error}"))?;
    stream.set_nodelay(true).ok();
    let _ = stream.set_read_timeout(Some(Duration::from_secs(25)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(25)));
    Ok(stream)
}
