//! Mail model and a local store. Sync and IMAP sit behind [`MailStore`].

mod model;
mod store;

pub use model::{Address, Mailbox, MailboxRole, Message, Thread};
pub use store::{MemoryStore, MailStore};
