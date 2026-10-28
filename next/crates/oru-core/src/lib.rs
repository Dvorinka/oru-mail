//! Mail model and a local store. Sync and IMAP sit behind [`MailStore`].

mod json;
mod model;
mod store;

pub use json::{error_json, mailboxes_json, parse_outbound, render_thread, thread_json, threads_json, Outbound};
pub use model::{Address, Mailbox, MailboxRole, Message, Thread};
pub use store::{MemoryStore, MailStore};
