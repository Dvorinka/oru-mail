import { useState } from 'react';
import { mailboxes, threads } from './mail.js';

export default function App() {
  const [mailboxId, setMailboxId] = useState('inbox');
  const [threadId, setThreadId] = useState('t1');
  const visible = threads.filter((thread) => thread.mailboxId === mailboxId);
  const selected = threads.find((thread) => thread.id === threadId) || visible[0] || null;

  function openMailbox(id) {
    setMailboxId(id);
    const next = threads.find((thread) => thread.mailboxId === id);
    setThreadId(next ? next.id : '');
  }

  return (
    <div className="app">
      <aside className="rail">
        <button className="compose" type="button">
          Compose
        </button>
        <nav>
          {mailboxes.map((mailbox) => (
            <button
              key={mailbox.id}
              type="button"
              className={mailbox.id === mailboxId ? 'box selected' : 'box'}
              onClick={() => openMailbox(mailbox.id)}
            >
              {mailbox.name}
            </button>
          ))}
        </nav>
      </aside>
      <section className="list">
        {visible.length === 0 ? (
          <p className="empty">Nothing in this box.</p>
        ) : (
          visible.map((thread) => (
            <button
              key={thread.id}
              type="button"
              className={selected && selected.id === thread.id ? 'row on' : 'row'}
              onClick={() => setThreadId(thread.id)}
            >
              <span className="avatar">{thread.from.slice(0, 1)}</span>
              <span className="row-copy">
                <span className="row-top">
                  <span className={thread.unread ? 'from unread' : 'from'}>{thread.from}</span>
                  <span className="when">{thread.when}</span>
                </span>
                <span className="subject">{thread.subject}</span>
                <span className="preview">{thread.preview}</span>
              </span>
              {thread.unread ? <i className="mark" /> : null}
            </button>
          ))
        )}
      </section>
      <article className="read">
        {selected ? (
          <>
            <h1>{selected.subject}</h1>
            <p className="who">{selected.from}</p>
            <div className="sheet">
              {selected.body.split('\n').map((line) => (
                <p key={line}>{line || '\u00a0'}</p>
              ))}
            </div>
            <div className="actions">
              <button type="button">Reply</button>
              <button type="button">Forward</button>
            </div>
          </>
        ) : (
          <p className="empty">Select a thread.</p>
        )}
      </article>
    </div>
  );
}
