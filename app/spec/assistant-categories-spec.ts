import { AccountStore, MailboxPerspective } from 'mailspring-exports';
import { validateFilter } from '../src/services/mail-access/filter';
import { filterForAssistantCategory } from '../src/assistant-categories';

describe('Assistant categories', function assistantCategories() {
  beforeEach(() => {
    spyOn(AccountStore, 'accounts').andReturn([{ id: 'a1' }, { id: 'a2' }]);
  });

  function sqlFor(id: string, accountIds = ['a1']) {
    const perspective = MailboxPerspective.forAssistantCategory(accountIds, id);
    return (perspective as any)._threadQuery().sql();
  }

  it('builds a filter for every sidebar category', () => {
    for (const category of MailboxPerspective.assistantCategories()) {
      expect(validateFilter(filterForAssistantCategory(category.id))).toBeTruthy();
      expect(category.name).toBeTruthy();
      expect(category.iconName).toBeTruthy();
    }
    expect(MailboxPerspective.assistantCategories().map((c) => c.name)).toEqual([
      'Subscriptions',
      'Travel',
      'Packages',
      'Bill & Receipts',
      'Entertainment',
    ]);
  });

  it('matches packages on shipping subjects and carrier domains only', () => {
    const sql = sqlFor('packages');
    expect(sql).toContain('shipped');
    expect(sql).toContain('ups.com');
    expect(sql).not.toContain('amazon.com');
    expect(sql).not.toContain('NOT');
    expect(sql).toContain('inAllMail');
  });

  it('gives travel priority over a package-shaped subject', () => {
    const sql = sqlFor('travel');
    expect(sql).toContain('united.com');
    expect(sql).toContain('boarding pass');
    expect(sql).toContain('(NOT ');
    expect(sql).toContain('shipped');
  });

  it('keeps list-unsubscribe mail in subscriptions only after specific categories', () => {
    const sql = sqlFor('subscriptions');
    expect(sql).toContain('hListUnsub');
    expect(sql).toContain('newsletter');
    expect(sql).toContain('netflix.com');
    expect(sql).toContain('(NOT ');
  });

  it('limits the query to the sidebar accounts when they are a subset', () => {
    const sql = sqlFor('bills', ['a1']);
    expect(sql).toContain('receipt');
    expect(sql).toContain("`accountId` = 'a1'");
  });

  it('does not add an account clause when every account is in view', () => {
    const sql = sqlFor('bills', ['a1', 'a2']);
    expect(sql).not.toContain('accountId');
  });

  it('round-trips through saved perspective JSON and distinguishes categories', () => {
    const travel = MailboxPerspective.forAssistantCategory(['a1'], 'travel');
    const bills = MailboxPerspective.forAssistantCategory(['a1'], 'bills');
    const restored = MailboxPerspective.fromJSON(travel.toJSON());

    expect(restored.isEqual(travel)).toBe(true);
    expect(travel.isEqual(bills)).toBe(false);
    expect(travel.isEqual(MailboxPerspective.forNothing())).toBe(false);
    expect(travel.canReceiveThreadsFromAccountIds(['a1'])).toBe(false);
    expect(
      MailboxPerspective.fromJSON({
        type: 'AssistantMailboxPerspective',
        accountIds: ['a1'],
        categoryId: 'nope',
      })
    ).toBe(null);
    expect(MailboxPerspective.forAssistantCategory(['a1'], 'nope')).toBe(null);
  });
});
