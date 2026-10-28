import React from 'react';
import {
  AccountStore,
  DatabaseStore,
  FocusedPerspectiveStore,
  Message,
  localized,
} from 'mailspring-exports';
import {
  FilterMatcher,
  compileFilter,
  validateFilter,
} from '../../../src/services/mail-access/filter';
import { MailingListRow, groupMailingListMessages } from './mailing-list-model';
import {
  getBestUnsubscribeOption,
  parseListUnsubscribeHeader,
  performMailtoUnsubscribe,
  performOneClickUnsubscribe,
  performWebUnsubscribe,
  supportsOneClick,
} from './unsubscribe-service';

type RowState = 'idle' | 'loading' | 'success' | 'error';

function messagesWithUnsubscribe(accountIds: string[]) {
  const sql = compileFilter('Message', validateFilter({ listUnsubscribe: true }), { accountIds });
  const query = DatabaseStore.findAll<Message>(Message)
    .where(new FilterMatcher(sql) as any)
    .order(Message.attributes.date.descending())
    .limit(800);
  if (accountIds.length > 0 && accountIds.length < AccountStore.accounts().length) {
    query.where(Message.attributes.accountId.in(accountIds));
  }
  return query;
}

/**
 * Senders that included a List-Unsubscribe header. One-click runs here;
 * mailto opens a draft and a web link opens in the browser.
 */
export default class MailingList extends React.Component<
  Record<string, never>,
  { rows: MailingListRow[]; status: Record<string, RowState>; error: Record<string, string> }
> {
  static displayName = 'MailingList';

  _mounted = false;
  _unlisten: () => void;

  state = { rows: [] as MailingListRow[], status: {} as Record<string, RowState>, error: {} };

  componentDidMount() {
    this._mounted = true;
    this._unlisten = FocusedPerspectiveStore.listen(this._reload);
    this._reload();
  }

  componentWillUnmount() {
    this._mounted = false;
    this._unlisten();
  }

  _reload = () => {
    const accountIds = FocusedPerspectiveStore.current().accountIds || [];
    messagesWithUnsubscribe(accountIds).then((messages) => {
      if (!this._mounted) {
        return;
      }
      this.setState({ rows: groupMailingListMessages(messages) });
    });
  };

  _unsubscribe = async (row: MailingListRow) => {
    const message = row.latest;
    const best = getBestUnsubscribeOption(
      parseListUnsubscribeHeader(message.listUnsubscribe),
      supportsOneClick(message.listUnsubscribePost)
    );
    if (!best) {
      this.setState((state) => ({
        status: { ...state.status, [row.email]: 'error' },
        error: { ...state.error, [row.email]: localized('No unsubscribe link') },
      }));
      return;
    }
    this.setState((state) => ({ status: { ...state.status, [row.email]: 'loading' } }));
    if (best.method === 'one-click') {
      const result = await performOneClickUnsubscribe(best.option.uri);
      if (!this._mounted) {
        return;
      }
      this.setState((state) => ({
        status: { ...state.status, [row.email]: result.success ? 'success' : 'error' },
        error: { ...state.error, [row.email]: result.error || '' },
      }));
      return;
    }
    if (best.method === 'mailto') {
      performMailtoUnsubscribe(best.option.uri);
    } else {
      performWebUnsubscribe(best.option.uri);
    }
    if (this._mounted) {
      this.setState((state) => ({ status: { ...state.status, [row.email]: 'success' } }));
    }
  };

  render() {
    const { rows, status } = this.state;
    return (
      <div className="mailing-list">
        <div className="mailing-list-title">{localized('Mailing Lists')}</div>
        {rows.length === 0 ? (
          <div className="mailing-list-empty">{localized('No mailing lists')}</div>
        ) : (
          <ul className="mailing-list-rows">
            {rows.map((row) => {
              const rowStatus = status[row.email] || 'idle';
              return (
                <li key={row.email} className="mailing-list-row">
                  <div className="mailing-list-sender">
                    <div className="mailing-list-name">{row.name}</div>
                    <div className="mailing-list-email">{row.email}</div>
                  </div>
                  <div className="mailing-list-count">{localized('%@ messages', row.count)}</div>
                  <button
                    className="btn btn-toolbar mailing-list-unsub"
                    disabled={rowStatus === 'loading' || rowStatus === 'success'}
                    onClick={() => this._unsubscribe(row)}
                  >
                    {rowStatus === 'success'
                      ? localized('Done')
                      : rowStatus === 'loading'
                        ? localized('Working…')
                        : localized('Unsubscribe')}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    );
  }
}
