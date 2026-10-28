/** Same demo mailbox as `MemoryStore::demo` in oru-core. */
export const mailboxes = [
  { id: 'inbox', name: 'Inbox' },
  { id: 'sent', name: 'Sent' },
  { id: 'archive', name: 'Archive' },
];

export const threads = [
  {
    id: 't1',
    mailboxId: 'inbox',
    from: 'Ada Marin',
    subject: 'Studio notes',
    preview: 'The rail stays dark. The canvas stays quiet.',
    body: 'The rail stays dark. The canvas stays quiet.\n\nReply when the type is set.',
    when: 'Tue',
    unread: true,
  },
  {
    id: 't2',
    mailboxId: 'inbox',
    from: 'Northwind',
    subject: 'Shipment 441',
    preview: 'Left the dock this morning.',
    body: 'Left the dock this morning. Tracking is on the invoice.',
    when: 'Mon',
    unread: false,
  },
  {
    id: 't3',
    mailboxId: 'sent',
    from: 'You',
    subject: 'Re: Fold sample',
    preview: 'Sending the square mark, not the round one.',
    body: 'Sending the square mark, not the round one. Lime on ink.',
    when: 'Sun',
    unread: false,
  },
];
