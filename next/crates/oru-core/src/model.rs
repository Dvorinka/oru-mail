/// A person on a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Address {
    pub name: String,
    pub email: String,
}

impl Address {
    pub fn new(name: impl Into<String>, email: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
        }
    }
}

/// Where a thread is filed. Custom is a user folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MailboxRole {
    Inbox,
    Sent,
    Drafts,
    Archive,
    Custom,
}

impl MailboxRole {
    /// Stable token in the JSON the shell reads.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inbox => "inbox",
            Self::Sent => "sent",
            Self::Drafts => "drafts",
            Self::Archive => "archive",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mailbox {
    pub id: String,
    pub name: String,
    pub role: MailboxRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    pub from: Address,
    /// Recipients. The shell shows the first address.
    pub to: Vec<Address>,
    pub subject: String,
    pub preview: String,
    pub body: String,
    /// Unix seconds. The store orders threads by the newest message.
    pub unix_secs: i64,
    pub unread: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    pub mailbox_id: String,
    pub subject: String,
    pub messages: Vec<Message>,
}

impl Thread {
    pub fn latest(&self) -> Option<&Message> {
        self.messages.iter().max_by_key(|message| message.unix_secs)
    }

    pub fn unread(&self) -> bool {
        self.messages.iter().any(|message| message.unread)
    }
}
