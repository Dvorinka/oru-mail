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

function guessHosts(email) {
  const domain = (email.split('@')[1] || '').toLowerCase();
  const known = {
    'gmail.com': ['imap.gmail.com', 'smtp.gmail.com'],
    'googlemail.com': ['imap.gmail.com', 'smtp.gmail.com'],
    'outlook.com': ['outlook.office365.com', 'smtp.office365.com'],
    'hotmail.com': ['outlook.office365.com', 'smtp.office365.com'],
    'live.com': ['outlook.office365.com', 'smtp.office365.com'],
    'yahoo.com': ['imap.mail.yahoo.com', 'smtp.mail.yahoo.com'],
    'icloud.com': ['imap.mail.me.com', 'smtp.mail.me.com'],
    'me.com': ['imap.mail.me.com', 'smtp.mail.me.com'],
  };
  const pair = known[domain] || [`imap.${domain}`, `smtp.${domain}`];
  return { imapHost: pair[0], imapPort: '993', smtpHost: pair[1], smtpPort: '587' };
}

export default function App() {
  const [mailboxes, setMailboxes] = useState([]);
  const [mailboxId, setMailboxId] = useState('inbox');
  const [threads, setThreads] = useState([]);
  const [threadId, setThreadId] = useState('');
  const [detail, setDetail] = useState(null);
  const [loaded, setLoaded] = useState(false);
  const [offline, setOffline] = useState(false);
  const [problem, setProblem] = useState('');
  const [compose, setCompose] = useState(false);
  const [draft, setDraft] = useState({ to: '', subject: '', body: '' });
  const [sending, setSending] = useState(false);
  const [mode, setMode] = useState('demo');
  const [email, setEmail] = useState('');
  const [accountOpen, setAccountOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [query, setQuery] = useState('');
  const [signIn, setSignIn] = useState({
    email: '',
    password: '',
    name: '',
    imapHost: '',
    imapPort: '993',
    smtpHost: '',
    smtpPort: '587',
  });

  useEffect(() => {
    let gone = false;
    getJson('/api/status')
      .then((status) => {
        if (gone) return;
        setMode(status.mode || 'demo');
        setEmail(status.email || '');
        if (status.error) setProblem(status.error);
      })
      .catch(() => {});
    getJson('/api/mailboxes')
      .then((rows) => {
        if (!gone) setMailboxes(rows);
      })
      .catch(() => {
        if (!gone) setOffline(true);
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

  useEffect(() => {
    if (!detail || detail.id !== threadId || !detail.unread) return;
    let gone = false;
    getJson('/api/threads/' + encodeURIComponent(threadId) + '/read', { method: 'POST' })
      .then(() => {
        if (gone) return;
        setThreads((rows) => rows.map((row) => (row.id === threadId ? { ...row, unread: false } : row)));
        setDetail((row) =>
          row && row.id === threadId
            ? { ...row, unread: false, messages: row.messages.map((message) => ({ ...message, unread: false })) }
            : row
        );
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
  }, [detail, threadId]);

  function openMailbox(id) {
    setCompose(false);
    setAccountOpen(false);
    setMailboxId(id);
  }

  async function reload(nextMailbox) {
    const boxes = await getJson('/api/mailboxes');
    setMailboxes(boxes);
    const preferred = nextMailbox || mailboxId;
    const box = boxes.some((item) => item.id === preferred) ? preferred : boxes[0] ? boxes[0].id : 'inbox';
    if (box !== mailboxId) setMailboxId(box);
    const rows = await getJson('/api/mailboxes/' + encodeURIComponent(box) + '/threads');
    setThreads(rows);
    setLoaded(true);
    setThreadId(rows[0] ? rows[0].id : '');
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

  async function signInSubmit(event) {
    event.preventDefault();
    setSyncing(true);
    setProblem('');
    try {
      const status = await getJson('/api/account', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(signIn),
      });
      setMode(status.mode || 'account');
      setEmail(status.email || signIn.email);
      setAccountOpen(false);
      if (status.error) setProblem(status.error);
      await reload('inbox');
    } catch (error) {
      setProblem(error.message);
      try {
        await reload('inbox');
      } catch {
        /* the banner already explains the failure */
      }
    } finally {
      setSyncing(false);
    }
  }

  async function syncNow() {
    setSyncing(true);
    setProblem('');
    try {
      const status = await getJson('/api/sync', { method: 'POST' });
      setMode(status.mode || mode);
      setEmail(status.email || email);
      if (status.error) setProblem(status.error);
      await reload();
    } catch (error) {
      setProblem(error.message);
    } finally {
      setSyncing(false);
    }
  }

  if (offline) {
    return (
      <div className="offline">
        <p className="mark-name">Oru</p>
        <p>Can’t reach the mail store. Start oru serve.</p>
      </div>
    );
  }

  const needle = query.trim().toLowerCase();
  const shown = threads.filter((thread) => {
    if (!needle) return true;
    const hay = [thread.from, thread.to, thread.subject, thread.preview].join(' ').toLowerCase();
    return hay.includes(needle);
  });
  const selected = shown.find((thread) => thread.id === threadId) || threads.find((thread) => thread.id === threadId) || null;
  const open = detail && detail.id === threadId ? detail : null;
  const panel = compose || accountOpen;

  return (
    <div className={panel ? 'app composing' : 'app'}>
      <aside className="rail">
        <div className="brand">
          <p className="mark-name">Oru</p>
          <p className="account">{mode === 'account' ? email : 'Sample mailbox'}</p>
        </div>
        <button
          className="compose"
          type="button"
          onClick={() => {
            setAccountOpen(false);
            setCompose(true);
          }}
        >
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
        <div className="rail-foot">
          {mode === 'account' ? (
            <button className="box" type="button" onClick={syncNow} disabled={syncing}>
              {syncing ? 'Syncing…' : 'Sync'}
            </button>
          ) : null}
          <button
            className="box"
            type="button"
            onClick={() => {
              setCompose(false);
              setAccountOpen(true);
            }}
          >
            {mode === 'account' ? 'Account' : 'Sign in'}
          </button>
        </div>
      </aside>
      <section className="list">
        <input
          className="search"
          placeholder="Search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        {!loaded ? (
          <p className="empty">Loading…</p>
        ) : shown.length === 0 ? (
          <p className="empty">{needle ? 'Nothing matches.' : 'Nothing in this box.'}</p>
        ) : (
          shown.map((thread) => (
            <button
              key={thread.id}
              type="button"
              className={selected && selected.id === thread.id ? 'row on' : 'row'}
              onClick={() => {
                setCompose(false);
                setAccountOpen(false);
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
        {accountOpen ? (
          <form className="form" onSubmit={signInSubmit}>
            <h1>Sign in</h1>
            <p className="note">
              Gmail and Outlook need an app password. It stays in the account file on this computer.
            </p>
            <label>
              Email
              <input
                autoFocus
                value={signIn.email}
                onChange={(event) => setSignIn({ ...signIn, email: event.target.value })}
                onBlur={() => {
                  if (!signIn.email.includes('@') || signIn.imapHost) return;
                  setSignIn({ ...signIn, ...guessHosts(signIn.email) });
                }}
              />
            </label>
            <label>
              Password
              <input
                type="password"
                value={signIn.password}
                onChange={(event) => setSignIn({ ...signIn, password: event.target.value })}
              />
            </label>
            <label>
              IMAP host
              <input
                value={signIn.imapHost}
                onChange={(event) => setSignIn({ ...signIn, imapHost: event.target.value })}
              />
            </label>
            <label>
              IMAP port
              <input
                value={signIn.imapPort}
                onChange={(event) => setSignIn({ ...signIn, imapPort: event.target.value })}
              />
            </label>
            <label>
              SMTP host
              <input
                value={signIn.smtpHost}
                onChange={(event) => setSignIn({ ...signIn, smtpHost: event.target.value })}
              />
            </label>
            <label>
              SMTP port
              <input
                value={signIn.smtpPort}
                onChange={(event) => setSignIn({ ...signIn, smtpPort: event.target.value })}
              />
            </label>
            <div className="actions">
              <button className="send" type="submit" disabled={syncing}>
                {syncing ? 'Signing in…' : 'Sign in'}
              </button>
              <button className="ghost" type="button" onClick={() => setAccountOpen(false)}>
                Cancel
              </button>
            </div>
          </form>
        ) : compose ? (
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
            {open ? (
              open.messages.map((message) => (
                <div key={message.id}>
                  <p className="who">{whoLine(message, mailboxId)}</p>
                  <div className="sheet">
                    {message.body.split('\n').map((line, index) => (
                      <p key={message.id + '-' + index}>{line || '\u00a0'}</p>
                    ))}
                  </div>
                </div>
              ))
            ) : (
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
                    setAccountOpen(false);
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
