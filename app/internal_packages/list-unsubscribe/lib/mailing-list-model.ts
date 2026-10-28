import { Message } from 'mailspring-exports';

export interface MailingListRow {
  email: string;
  name: string;
  count: number;
  latest: Message;
}

function fromContact(message: Message) {
  const from = message.from && message.from[0];
  if (!from || !from.email) {
    return null;
  }
  return { email: from.email.toLowerCase(), name: from.name || from.email };
}

function timeOf(message: Message) {
  const date = message.date as Date | number | string | undefined;
  if (!date) {
    return 0;
  }
  const value = date instanceof Date ? date.getTime() : new Date(date).getTime();
  return Number.isNaN(value) ? 0 : value;
}

/** One row per sender. The newest message keeps the unsubscribe headers. */
export function groupMailingListMessages(messages: Message[]): MailingListRow[] {
  const byEmail = new Map<string, MailingListRow>();
  for (const message of messages) {
    const from = fromContact(message);
    if (!from) {
      continue;
    }
    const existing = byEmail.get(from.email);
    if (!existing) {
      byEmail.set(from.email, {
        email: from.email,
        name: from.name,
        count: 1,
        latest: message,
      });
      continue;
    }
    existing.count += 1;
    if (timeOf(message) >= timeOf(existing.latest)) {
      existing.latest = message;
      if (from.name) {
        existing.name = from.name;
      }
    }
  }
  return Array.from(byEmail.values()).sort((a, b) => timeOf(b.latest) - timeOf(a.latest));
}
