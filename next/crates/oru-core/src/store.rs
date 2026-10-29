use std::time::{SystemTime, UNIX_EPOCH};

use crate::{Address, Mailbox, MailboxRole, Message, Thread};

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
    next_id: u64,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_mailbox(&mut self, mailbox: Mailbox) {
        self.mailboxes.push(mailbox);
    }

    pub fn insert_thread(&mut self, thread: Thread) {
        if self.threads.iter().any(|existing| existing.id == thread.id) {
            return;
        }
        self.threads.push(thread);
    }

    /// Smallest IMAP uid already stored in this mailbox. Demo ids have no uid.
    pub fn oldest_uid(&self, mailbox_id: &str) -> Option<u32> {
        self.threads
            .iter()
            .filter(|thread| thread.mailbox_id == mailbox_id)
            .flat_map(|thread| thread.messages.iter())
            .filter_map(|message| message.id.split_once(':').and_then(|(_, uid)| uid.parse().ok()))
            .min()
    }

    /// Demo mailbox the CLI and the shell read through the JSON API.
    pub fn demo() -> Self {
        let mut store = Self::new();
        store.insert_mailbox(Mailbox {
            id: "inbox".into(),
            name: "Inbox".into(),
            role: MailboxRole::Inbox,
        });
        store.insert_mailbox(Mailbox {
            id: "sent".into(),
            name: "Sent".into(),
            role: MailboxRole::Sent,
        });
        store.insert_mailbox(Mailbox {
            id: "archive".into(),
            name: "Archive".into(),
            role: MailboxRole::Archive,
        });
        let you = Address::new("You", "you@oru.test");
        store.insert_thread(Thread {
            id: "t1".into(),
            mailbox_id: "inbox".into(),
            subject: "Studio notes".into(),
            messages: vec![Message {
                id: "m1".into(),
                message_id: "m1@studio.test".into(),
                from: Address::new("Ada Marin", "ada@studio.test"),
                to: vec![you.clone()],
                subject: "Studio notes".into(),
                preview: "The rail stays dark. The canvas stays quiet.".into(),
                body: "The rail stays dark. The canvas stays quiet.\n\nReply when the type is set.".into(),
                html: String::new(),
                attachments: Vec::new(),
                unix_secs: 1_730_000_000,
                unread: true,
                starred: false,
            }],
        });
        store.insert_thread(Thread {
            id: "t2".into(),
            mailbox_id: "inbox".into(),
            subject: "Shipment 441".into(),
            messages: vec![Message {
                id: "m2".into(),
                message_id: "m2@northwind.test".into(),
                from: Address::new("Northwind", "desk@northwind.test"),
                to: vec![you.clone()],
                subject: "Shipment 441".into(),
                preview: "Left the dock this morning.".into(),
                body: "Left the dock this morning. Tracking is on the invoice.".into(),
                html: String::new(),
                attachments: vec![crate::Attachment {
                    id: "m2:0".into(),
                    name: "invoice.txt".into(),
                    mime: "text/plain".into(),
                    bytes: b"Tracking is on the invoice.\n".to_vec(),
                }],
                unix_secs: 1_729_900_000,
                unread: false,
                starred: false,
            }],
        });
        store.insert_thread(Thread {
            id: "t3".into(),
            mailbox_id: "sent".into(),
            subject: "Re: Fold sample".into(),
            messages: vec![Message {
                id: "m3".into(),
                message_id: "m3@oru.test".into(),
                from: you,
                to: vec![Address::new("Ada Marin", "ada@studio.test")],
                subject: "Re: Fold sample".into(),
                preview: "Sending the square mark, not the round one.".into(),
                body: "Sending the square mark, not the round one. Lime on ink.".into(),
                html: "<p>Sending the square mark, not the round one.</p><script>alert(1)</script>".into(),
                attachments: Vec::new(),
                unix_secs: 1_729_800_000,
                unread: false,
                starred: false,
            }],
        });
        store
    }

    /// Files a message in Sent. The id is local to this process.
    pub fn send(&mut self, to: &str, subject: &str, body: &str) -> Thread {
        self.deliver(Address::new("You", "you@oru.test"), to, subject, body)
    }

    pub fn deliver(&mut self, from: Address, to: &str, subject: &str, body: &str) -> Thread {
        if !self.mailboxes.iter().any(|mailbox| mailbox.id == "sent") {
            self.insert_mailbox(Mailbox {
                id: "sent".into(),
                name: "Sent".into(),
                role: MailboxRole::Sent,
            });
        }
        self.next_id += 1;
        let id = self.next_id;
        let preview = preview_line(body);
        let thread = Thread {
            id: format!("n{id}"),
            mailbox_id: "sent".into(),
            subject: subject.to_string(),
            messages: vec![Message {
                id: format!("nm{id}"),
                message_id: format!("nm{id}@oru.local"),
                from,
                to: vec![Address::new(to, to)],
                subject: subject.to_string(),
                preview,
                body: body.to_string(),
                html: String::new(),
                attachments: Vec::new(),
                unix_secs: now_secs(),
                unread: false,
                starred: false,
            }],
        };
        self.insert_thread(thread.clone());
        thread
    }

    /// Clears the unread flag. The shell does this when a thread is opened.
    pub fn mark_read(&mut self, thread_id: &str) -> bool {
        let Some(thread) = self.threads.iter_mut().find(|thread| thread.id == thread_id) else {
            return false;
        };
        for message in &mut thread.messages {
            message.unread = false;
        }
        true
    }

    /// Marks every message unread. The latest one is enough for the row mark.
    pub fn mark_unread(&mut self, thread_id: &str) -> bool {
        let Some(thread) = self.threads.iter_mut().find(|thread| thread.id == thread_id) else {
            return false;
        };
        for message in &mut thread.messages {
            message.unread = true;
        }
        true
    }

    /// A thread with any starred message becomes unstarred. Otherwise every message is starred.
    pub fn toggle_star(&mut self, thread_id: &str) -> Option<bool> {
        let thread = self.threads.iter_mut().find(|thread| thread.id == thread_id)?;
        let next = !thread.messages.iter().any(|message| message.starred);
        for message in &mut thread.messages {
            message.starred = next;
        }
        Some(next)
    }

    pub fn move_thread(&mut self, thread_id: &str, mailbox_id: &str) -> bool {
        if !self.mailboxes.iter().any(|mailbox| mailbox.id == mailbox_id) {
            return false;
        }
        let Some(thread) = self.threads.iter_mut().find(|thread| thread.id == thread_id) else {
            return false;
        };
        thread.mailbox_id = mailbox_id.to_string();
        true
    }

    pub fn remove_thread(&mut self, thread_id: &str) -> bool {
        let before = self.threads.len();
        self.threads.retain(|thread| thread.id != thread_id);
        self.threads.len() != before
    }

    pub fn attachment(&self, id: &str) -> Option<&crate::Attachment> {
        let (message_id, index) = id.rsplit_once(':')?;
        let index: usize = index.parse().ok()?;
        self.threads.iter().find_map(|thread| {
            thread.messages.iter().find(|message| message.id == message_id).and_then(|message| message.attachments.get(index))
        })
    }
}

fn preview_line(body: &str) -> String {
    let line = body.lines().find(|line| !line.trim().is_empty()).unwrap_or("");
    let mut preview: String = line.chars().take(120).collect();
    if line.chars().count() > 120 {
        preview.push('…');
    }
    preview
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
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
        let mut store = MemoryStore::demo();
        let threads = store.threads("inbox");
        assert_eq!(threads.len(), 2);
        assert_eq!(threads[0].id, "t1");
        assert!(threads[0].unread());
        assert!(!threads[1].unread());
        assert!(store.threads("archive").is_empty());
        assert_eq!(store.threads("sent").len(), 1);
        assert_eq!(store.thread("t2").unwrap().subject, "Shipment 441");
        assert_eq!(store.oldest_uid("inbox"), None);
        store.insert_thread(Thread {
            id: "inbox:9".into(),
            mailbox_id: "inbox".into(),
            subject: "Newer".into(),
            messages: vec![Message {
                id: "inbox:9".into(),
                message_id: "9@studio.test".into(),
                from: Address::new("Ada", "ada@studio.test"),
                to: vec![],
                subject: "Newer".into(),
                preview: String::new(),
                body: String::new(),
                html: String::new(),
                attachments: Vec::new(),
                unix_secs: 10,
                unread: false,
                starred: false,
            }],
        });
        store.insert_thread(Thread {
            id: "inbox:2".into(),
            mailbox_id: "inbox".into(),
            subject: "Older".into(),
            messages: vec![Message {
                id: "inbox:2".into(),
                message_id: "2@studio.test".into(),
                from: Address::new("Ada", "ada@studio.test"),
                to: vec![],
                subject: "Older".into(),
                preview: String::new(),
                body: String::new(),
                html: String::new(),
                attachments: Vec::new(),
                unix_secs: 1,
                unread: false,
                starred: false,
            }],
        });
        store.insert_thread(Thread {
            id: "inbox:2".into(),
            mailbox_id: "inbox".into(),
            subject: "Duplicate".into(),
            messages: vec![],
        });
        assert!(!store.thread("t2").unwrap().starred());
        assert_eq!(store.toggle_star("t2"), Some(true));
        assert!(store.thread("t2").unwrap().messages.iter().all(|message| message.starred));
        assert_eq!(store.toggle_star("t2"), Some(false));
        assert_eq!(store.toggle_star("missing"), None);
        assert_eq!(store.oldest_uid("inbox"), Some(2));
        assert_eq!(store.threads("inbox").iter().filter(|thread| thread.id == "inbox:2").count(), 1);
    }

    #[test]
    fn send_files_the_message_in_sent() {
        let mut store = MemoryStore::demo();
        let thread = store.send("ada@studio.test", "Fold", "Lime on ink");
        assert_eq!(thread.mailbox_id, "sent");
        assert_eq!(thread.messages[0].to[0].email, "ada@studio.test");
        assert!(store.threads("sent").iter().any(|item| item.id == thread.id));
        assert_ne!(thread.id, "t3");
    }
}
