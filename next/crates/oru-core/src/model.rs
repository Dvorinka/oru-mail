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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// A file on a message. The bytes stay on the store; the JSON lists name and size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub id: String,
    /// Message-ID without brackets. Reply uses it as In-Reply-To.
    pub message_id: String,
    pub from: Address,
    /// Recipients. The shell shows the first address.
    pub to: Vec<Address>,
    pub subject: String,
    pub preview: String,
    pub body: String,
    /// Sanitized HTML. Empty when the message is plain text.
    pub html: String,
    pub attachments: Vec<Attachment>,
    /// Unix seconds. The store orders threads by the newest message.
    pub unix_secs: i64,
    pub unread: bool,
    pub starred: bool,
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

    pub fn starred(&self) -> bool {
        self.messages.iter().any(|message| message.starred)
    }
}
