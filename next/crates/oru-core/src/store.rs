use crate::{Mailbox, Message, Thread};

/// Read side of a mailbox. The Electron app keeps using Mailspring-Sync.
/// This trait is the seam the desktop, web, and phone shells call.
pub trait MailStore {
    fn mailboxes(&self) -> Vec<Mailbox>;
    fn threads(&self, mailbox_id: &str) -> Vec<Thread>;
    fn thread(&self, thread_id: &str) -> Option<Thread>;
}

#[derive(Default)]
pub struct MemoryStore {
    mailboxes: Vec<Mailbox>,
    threads: Vec<Thread>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_mailbox(&mut self, mailbox: Mailbox) {
        self.mailboxes.push(mailbox);
    }

    pub fn insert_thread(&mut self, thread: Thread) {
        self.threads.push(thread);
    }

    /// Demo mailbox used by the CLI and the shell fixture.
    pub fn demo() -> Self {
        let mut store = Self::new();
        store.insert_mailbox(Mailbox {
            id: "inbox".into(),
            name: "Inbox".into(),
            role: crate::MailboxRole::Inbox,
        });
        store.insert_mailbox(Mailbox {
            id: "archive".into(),
            name: "Archive".into(),
            role: crate::MailboxRole::Archive,
        });
        store.insert_thread(Thread {
            id: "t1".into(),
            mailbox_id: "inbox".into(),
            subject: "Studio notes".into(),
            messages: vec![Message {
                id: "m1".into(),
                from: crate::Address::new("Ada Marin", "ada@studio.test"),
                subject: "Studio notes".into(),
                preview: "The rail stays dark. The canvas stays quiet.".into(),
                body: "The rail stays dark. The canvas stays quiet.\n\nReply when the type is set.".into(),
                unix_secs: 1_730_000_000,
                unread: true,
            }],
        });
        store.insert_thread(Thread {
            id: "t2".into(),
            mailbox_id: "inbox".into(),
            subject: "Shipment 441".into(),
            messages: vec![Message {
                id: "m2".into(),
                from: crate::Address::new("Northwind", "desk@northwind.test"),
                subject: "Shipment 441".into(),
                preview: "Left the dock this morning.".into(),
                body: "Left the dock this morning. Tracking is on the invoice.".into(),
                unix_secs: 1_729_900_000,
                unread: false,
            }],
        });
        store
    }
}

impl MailStore for MemoryStore {
    fn mailboxes(&self) -> Vec<Mailbox> {
        self.mailboxes.clone()
    }

    fn threads(&self, mailbox_id: &str) -> Vec<Thread> {
        let mut found: Vec<Thread> = self
            .threads
            .iter()
            .filter(|thread| thread.mailbox_id == mailbox_id)
            .cloned()
            .collect();
        found.sort_by(|a, b| {
            let left = a.latest().map(|message| message.unix_secs).unwrap_or(0);
            let right = b.latest().map(|message| message.unix_secs).unwrap_or(0);
            right.cmp(&left)
        });
        found
    }

    fn thread(&self, thread_id: &str) -> Option<Thread> {
        self.threads.iter().find(|thread| thread.id == thread_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inbox_is_newest_first_and_unread_follows_the_message() {
        let store = MemoryStore::demo();
        let threads = store.threads("inbox");
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].id, "t1");
        assert!(threads[0].unread());
        assert!(!threads[1].unread());
        assert!(store.threads("archive").is_empty());
        assert_eq!(store.thread("t2").unwrap().subject, "Shipment 441");
    }
}
