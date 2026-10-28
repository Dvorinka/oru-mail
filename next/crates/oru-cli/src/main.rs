use oru_core::{MailStore, MemoryStore};

fn main() {
    let store = MemoryStore::demo();
    for mailbox in store.mailboxes() {
        println!("{}", mailbox.name);
        for thread in store.threads(&mailbox.id) {
            let mark = if thread.unread() { "*" } else { " " };
            let from = thread
                .latest()
                .map(|message| message.from.name.clone())
                .unwrap_or_default();
            println!("  {mark} {from} — {}", thread.subject);
        }
    }
}
