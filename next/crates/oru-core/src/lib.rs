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
    append_sent, begin_starttls, copy_away, pull_before, pull_matching, pull_session, raw_message, raw_message_attached,
    read_greeting, set_deleted,
    set_flagged,
    set_seen,
    set_unseen,
    Synced,
};
pub use json::{
    error_json, json_string, mailboxes_json, parse_draft, parse_outbound, parse_search, render_thread, thread_json,
    thread_list_json,
    threads_json,
    Outbound, OutboundFile,
};
pub use model::{Address, Attachment, Mailbox, MailboxRole, Message, Thread};
pub use smtp::{prepare_starttls, send_mail, Outgoing};
pub use store::{MailStore, MemoryStore};
