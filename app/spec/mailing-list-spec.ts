import { Message } from 'mailspring-exports';
import { groupMailingListMessages } from '../internal_packages/list-unsubscribe/lib/mailing-list-model';
import { MailboxPerspective, MailingListMailboxPerspective } from '../src/mailbox-perspective';

describe('Mailing lists', function describeMailingLists() {
  it('groups messages by sender and keeps the newest headers', () => {
    const older = new Message({
      id: 'm1',
      from: [{ email: 'News@Example.com', name: 'News' }],
      date: new Date('2026-01-01T00:00:00Z'),
      listUnsubscribe: '<mailto:old@example.com>',
    });
    const newer = new Message({
      id: 'm2',
      from: [{ email: 'news@example.com', name: 'Example News' }],
      date: new Date('2026-02-01T00:00:00Z'),
      listUnsubscribe: '<https://example.com/unsub>',
    });
    const other = new Message({
      id: 'm3',
      from: [{ email: 'billing@shop.test', name: 'Shop' }],
      date: new Date('2026-01-15T00:00:00Z'),
    });

    const rows = groupMailingListMessages([older, other, newer]);

    expect(rows.length).toBe(2);
    expect(rows[0].email).toBe('news@example.com');
    expect(rows[0].name).toBe('Example News');
    expect(rows[0].count).toBe(2);
    expect(rows[0].latest.id).toBe('m2');
    expect(rows[1].email).toBe('billing@shop.test');
    expect(rows[1].count).toBe(1);
  });

  it('skips messages with no sender', () => {
    const rows = groupMailingListMessages([new Message({ id: 'm', from: [] })]);
    expect(rows).toEqual([]);
  });

  it('restores from JSON and does not query the thread list', () => {
    const perspective = MailboxPerspective.forMailingLists(['a1']);
    expect(perspective instanceof MailingListMailboxPerspective).toBe(true);
    expect(perspective.threads()).toBe(null);
    expect(perspective.canReceiveThreadsFromAccountIds(['a1'])).toBe(false);

    const restored = MailboxPerspective.fromJSON(perspective.toJSON());
    expect(restored.isEqual(perspective)).toBe(true);
    expect(restored.isEqual(MailboxPerspective.forMailingLists(['a2']))).toBe(false);
  });
});
