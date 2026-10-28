import { useEffect, useState } from 'react';

async function getJson(path, options) {
  const response = await fetch(path, options);
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || 'The mail store returned ' + response.status);
  }
  return payload;
}

function whenLabel(unixSecs) {
  if (!unixSecs) return '';
  return new Date(unixSecs * 1000).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

function party(item, mailboxId) {
  const name = mailboxId === 'sent' ? item.to || item.toEmail || item.from : item.from;
  return name || '';
}

function whoLine(message, mailboxId) {
  const name = party(message, mailboxId);
  const email = mailboxId === 'sent' ? message.toEmail : message.fromEmail;
  if (!email || email === name) return name;
  return name + ' · ' + email;
}

export default function App() {
  const [mailboxes, setMailboxes] = useState([]);
  const [mailboxId, setMailboxId] = useState('inbox');
  const [threads, setThreads] = useState([]);
  const [threadId, setThreadId] = useState('');
  const [detail, setDetail] = useState(null);
  const [loaded, setLoaded] = useState(false);
  const [problem, setProblem] = useState('');
  const [compose, setCompose] = useState(false);
  const [draft, setDraft] = useState({ to: '', subject: '', body: '' });
  const [sending, setSending] = useState(false);

  useEffect(() => {
    let gone = false;
    getJson('/api/mailboxes')
      .then((rows) => {
        if (!gone) setMailboxes(rows);
      })
      .catch(() => {
        if (!gone) setProblem('Can’t reach the mail store. Start oru serve.');
      });
    return () => {
      gone = true;
    };
  }, []);

  useEffect(() => {
    let gone = false;
    setLoaded(false);
    getJson('/api/mailboxes/' + encodeURIComponent(mailboxId) + '/threads')
      .then((rows) => {
        if (gone) return;
        setThreads(rows);
        setLoaded(true);
        setThreadId((current) => (rows.some((row) => row.id === current) ? current : rows[0] ? rows[0].id : ''));
      })
      .catch(() => {
        if (!gone) setProblem('Could not load this mailbox.');
      });
    return () => {
      gone = true;
    };
  }, [mailboxId]);

  useEffect(() => {
    if (!threadId) {
      setDetail(null);
      return;
    }
    let gone = false;
    getJson('/api/threads/' + encodeURIComponent(threadId))
      .then((row) => {
        if (!gone) setDetail(row);
      })
      .catch(() => {
        if (!gone) setDetail(null);
      });
    return () => {
      gone = true;
    };
  }, [threadId]);

  function openMailbox(id) {
    setCompose(false);
    setMailboxId(id);
  }

  async function submit(event) {
    event.preventDefault();
    setSending(true);
    setProblem('');
    try {
      const created = await getJson('/api/messages', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(draft),
      });
      const rows = await getJson('/api/mailboxes/sent/threads');
      setThreads(rows);
      setLoaded(true);
      setDraft({ to: '', subject: '', body: '' });
      setCompose(false);
      setMailboxId('sent');
      setThreadId(created.id);
    } catch (error) {
      setProblem(error.message);
    } finally {
      setSending(false);
    }
  }

  if (problem && mailboxes.length === 0) {
    return (
      <div className="offline">
        <p className="mark-name">Oru</p>
        <p>{problem}</p>
      </div>
    );
  }

  const selected = threads.find((thread) => thread.id === threadId) || null;
  const open = detail && detail.id === threadId ? detail : null;

  return (
    <div className={compose ? 'app composing' : 'app'}>
      <aside className="rail">
        <div className="brand">
          <p className="mark-name">Oru</p>
          <p className="account">you@oru.test</p>
        </div>
        <button className="compose" type="button" onClick={() => setCompose(true)}>
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
        {!loaded ? (
          <p className="empty">Loading…</p>
        ) : threads.length === 0 ? (
          <p className="empty">Nothing in this box.</p>
        ) : (
          threads.map((thread) => (
            <button
              key={thread.id}
              type="button"
              className={selected && selected.id === thread.id ? 'row on' : 'row'}
              onClick={() => {
                setCompose(false);
                setThreadId(thread.id);
              }}
            >
              <span className="avatar">{party(thread, mailboxId).slice(0, 1)}</span>
              <span className="row-copy">
                <span className="row-top">
                  <span className={thread.unread ? 'from unread' : 'from'}>{party(thread, mailboxId)}</span>
                  <span className="when">{whenLabel(thread.unixSecs)}</span>
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
        {problem ? <p className="problem">{problem}</p> : null}
        {compose ? (
          <form className="form" onSubmit={submit}>
            <h1>New message</h1>
            <label>
              To
              <input
                autoFocus
                value={draft.to}
                onChange={(event) => setDraft({ ...draft, to: event.target.value })}
              />
            </label>
            <label>
              Subject
              <input
                value={draft.subject}
                onChange={(event) => setDraft({ ...draft, subject: event.target.value })}
              />
            </label>
            <label>
              Message
              <textarea
                value={draft.body}
                onChange={(event) => setDraft({ ...draft, body: event.target.value })}
              />
            </label>
            <div className="actions">
              <button className="send" type="submit" disabled={sending}>
                {sending ? 'Sending…' : 'Send'}
              </button>
              <button className="ghost" type="button" onClick={() => setCompose(false)}>
                Cancel
              </button>
            </div>
          </form>
        ) : selected ? (
          <>
            <h1>{(open || selected).subject}</h1>
            {open ? open.messages.map((message) => (
              <div key={message.id}>
                <p className="who">{whoLine(message, mailboxId)}</p>
                <div className="sheet">
                  {message.body.split('\n').map((line, index) => (
                    <p key={message.id + '-' + index}>{line || '\u00a0'}</p>
                  ))}
                </div>
              </div>
            )) : (
              <p className="empty">Loading…</p>
            )}
            {open ? (
              <div className="actions">
                <button
                  type="button"
                  onClick={() => {
                    setDraft({
                      to: mailboxId === 'sent' ? open.toEmail || '' : open.fromEmail || '',
                      subject: open.subject.startsWith('Re:') ? open.subject : 'Re: ' + open.subject,
                      body: '',
                    });
                    setCompose(true);
                  }}
                >
                  Reply
                </button>
              </div>
            ) : null}
          </>
        ) : (
          <p className="empty">Select a thread.</p>
        )}
      </article>
    </div>
  );
}
