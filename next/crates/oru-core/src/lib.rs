//! Mail model, JSON, and the IMAP/SMTP session.
//! The session speaks the protocol over a byte stream. TLS stays in the binary.

mod account;
mod base64;
mod imap;
mod json;
mod mime;
mod model;
mod smtp;
mod store;

pub use account::{account_json, guess_hosts, parse_account, Account};
pub use imap::{
    append_sent, begin_starttls, copy_away, pull_before, pull_session, raw_message, read_greeting, set_deleted, set_seen,
    set_unseen,
    Synced,
};
pub use json::{
    error_json, json_string, mailboxes_json, parse_outbound, render_thread, thread_json, threads_json, Outbound,
};
pub use model::{Address, Attachment, Mailbox, MailboxRole, Message, Thread};
pub use smtp::{prepare_starttls, send_mail, Outgoing};
pub use store::{MailStore, MemoryStore};
