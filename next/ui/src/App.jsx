import { useEffect, useRef, useState } from 'react';

async function getJson(path, options) {
  const response = await fetch(path, options);
  const payload = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(payload.error || 'The mail store returned ' + response.status);
  }
  return payload;
}

function sizeLabel(bytes) {
  if (!bytes) return '';
  if (bytes < 1024) return ' ' + bytes + ' B';
  return ' ' + Math.max(1, Math.round(bytes / 1024)) + ' KB';
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

function listPath(id) {
  return id === 'starred' ? '/api/starred' : '/api/mailboxes/' + encodeURIComponent(id) + '/threads';
}

function searchPath(id) {
  return id === 'starred' ? '/api/starred/search' : '/api/mailboxes/' + encodeURIComponent(id) + '/search';
}

function oldestUid(rows) {
  let oldest = null;
  for (const row of rows) {
    const uid = Number(String(row.id).split(':')[1]);
    if (Number.isInteger(uid) && uid > 0 && (oldest === null || uid < oldest)) oldest = uid;
  }
  return oldest;
}

const emptyDraft = { to: '', subject: '', body: '', inReplyTo: '', kind: '', fileName: '', fileMime: '', fileData: '' };

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
  const [draft, setDraft] = useState(emptyDraft);
  const seen = useRef('');
  const [sending, setSending] = useState(false);
  const [mode, setMode] = useState('demo');
  const [email, setEmail] = useState('');
  const [accountOpen, setAccountOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [loadingOlder, setLoadingOlder] = useState(false);
  const [olderNote, setOlderNote] = useState('');
  const [query, setQuery] = useState('');
  const [moveTo, setMoveTo] = useState('');
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
    const needle = query.trim();
    if (!needle) setLoaded(false);
    const timer = setTimeout(() => {
      const path = needle ? searchPath(mailboxId) : listPath(mailboxId);
      const options = needle
        ? {
            method: 'POST',
            headers: { 'content-type': 'application/json' },
            body: JSON.stringify({ q: needle }),
          }
        : undefined;
      getJson(path, options)
        .then((rows) => {
          if (gone) return;
          setThreads(rows);
          setLoaded(true);
          setOlderNote('');
          setThreadId((current) => (rows.some((row) => row.id === current) ? current : rows[0] ? rows[0].id : ''));
        })
        .catch(() => {
          if (!gone) {
            setLoaded(true);
            setProblem(needle ? 'Search failed.' : 'Could not load this mailbox.');
          }
        });
    }, needle ? 250 : 0);
    return () => {
      gone = true;
      clearTimeout(timer);
    };
  }, [mailboxId, query]);

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
    if (seen.current === threadId || seen.current === 'keep:' + threadId) return;
    seen.current = threadId;
    let gone = false;
    const id = threadId;
    getJson('/api/threads/' + encodeURIComponent(id) + '/read', { method: 'POST' })
      .then(() => {
        if (gone || seen.current !== id) return;
        setThreads((rows) => rows.map((row) => (row.id === id ? { ...row, unread: false } : row)));
        setDetail((row) =>
          row && row.id === id
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
    setOlderNote('');
    setQuery('');
    setMailboxId(id);
  }

  async function loadOlder() {
    setLoadingOlder(true);
    setProblem('');
    setOlderNote('');
    const needle = query.trim();
    try {
      if (needle) {
        const before = oldestUid(threads);
        if (!before) {
          setOlderNote('No older mail.');
          return;
        }
        const rows = await getJson(searchPath(mailboxId), {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ q: needle, before: String(before) }),
        });
        if (rows.length === 0) setOlderNote('No older mail.');
        else {
          setThreads((current) => {
            const seen = new Set(current.map((row) => row.id));
            return current.concat(rows.filter((row) => !seen.has(row.id)));
          });
        }
        return;
      }
      const rows = await getJson('/api/mailboxes/' + encodeURIComponent(mailboxId) + '/older', { method: 'POST' });
      setThreads(rows);
      setLoaded(true);
      if (rows.length === threads.length) setOlderNote('No older mail.');
    } catch (error) {
      setProblem(error.message);
    } finally {
      setLoadingOlder(false);
    }
  }

  async function reload(nextMailbox) {
    setQuery('');
    const boxes = await getJson('/api/mailboxes');
    setMailboxes(boxes);
    const preferred = nextMailbox || mailboxId;
    if (preferred === 'starred') {
      if (mailboxId !== 'starred') setMailboxId('starred');
      const rows = await getJson('/api/starred');
      setThreads(rows);
      setLoaded(true);
      setThreadId(rows[0] ? rows[0].id : '');
      return;
    }
    const box = boxes.some((item) => item.id === preferred) ? preferred : boxes[0] ? boxes[0].id : 'inbox';
    if (box !== mailboxId) setMailboxId(box);
    const rows = await getJson('/api/mailboxes/' + encodeURIComponent(box) + '/threads');
    setThreads(rows);
    setLoaded(true);
    setThreadId(rows[0] ? rows[0].id : '');
  }

  async function saveDraft() {
    setSending(true);
    setProblem('');
    try {
      const created = await getJson('/api/drafts', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          to: draft.to,
          subject: draft.subject,
          body: draft.body,
          inReplyTo: draft.inReplyTo,
          fileName: draft.fileName,
          fileMime: draft.fileMime,
          fileData: draft.fileData,
        }),
      });
      setDraft(emptyDraft);
      setCompose(false);
      await reload('drafts');
      if (created.id) setThreadId(created.id);
    } catch (error) {
      setProblem(error.message);
    } finally {
      setSending(false);
    }
  }

  async function submit(event) {
    event.preventDefault();
    setSending(true);
    setProblem('');
    try {
      const created = await getJson('/api/messages', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          to: draft.to,
          subject: draft.subject,
          body: draft.body,
          inReplyTo: draft.inReplyTo,
          fileName: draft.fileName,
          fileMime: draft.fileMime,
          fileData: draft.fileData,
        }),
      });
      const rows = await getJson('/api/mailboxes/sent/threads');
      setThreads(rows);
      setLoaded(true);
      setDraft(emptyDraft);
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

  function latestMessage(thread) {
    return thread.messages.reduce((best, message) => (message.unixSecs >= best.unixSecs ? message : best));
  }

  function quotedBody(message) {
    return (message.body || '')
      .split('\n')
      .map((line) => '> ' + line)
      .join('\n');
  }

  function replyTo(thread) {
    if (!thread.messages || thread.messages.length === 0) return;
    const message = latestMessage(thread);
    const quoted = quotedBody(message);
    const subject = thread.subject.startsWith('Re:') ? thread.subject : 'Re: ' + thread.subject;
    setDraft({
      ...emptyDraft,
      to: mailboxId === 'sent' ? thread.toEmail || message.toEmail || '' : thread.fromEmail || message.fromEmail || '',
      subject,
      body: quoted.trim() ? '\n\n' + quoted : '',
      inReplyTo: message.messageId || '',
      kind: 'reply',
    });
    setAccountOpen(false);
    setCompose(true);
  }

  function forwardMessage(thread) {
    if (!thread.messages || thread.messages.length === 0) return;
    const message = latestMessage(thread);
    const quoted = quotedBody(message);
    const subject = thread.subject.startsWith('Fwd:') ? thread.subject : 'Fwd: ' + thread.subject;
    const from = message.fromEmail
      ? (message.from || message.fromEmail) + ' <' + message.fromEmail + '>'
      : message.from || '';
    const header = 'Forwarded message\nFrom: ' + from + '\nSubject: ' + (message.subject || thread.subject || '');
    setDraft({
      ...emptyDraft,
      subject,
      body: header + (quoted.trim() ? '\n\n' + quoted : ''),
      kind: 'forward',
    });
    setAccountOpen(false);
    setCompose(true);
  }

  async function changeThread(kind) {
    setProblem('');
    if (kind === 'unread') seen.current = 'keep:' + threadId;
    try {
      const updated = await getJson('/api/threads/' + encodeURIComponent(threadId) + '/' + kind, { method: 'POST' });
      if (kind === 'trash' || kind === 'archive') {
        await reload(mailboxId);
        return;
      }
      if (kind === 'star') {
        const starred = !!(updated && updated.starred);
        setThreads((rows) =>
          mailboxId === 'starred' && !starred
            ? rows.filter((row) => row.id !== threadId)
            : rows.map((row) => (row.id === threadId ? { ...row, starred } : row))
        );
        setDetail((row) => (row && row.id === threadId ? updated : row));
        return;
      }
      setThreads((rows) => rows.map((row) => (row.id === threadId ? { ...row, unread: true } : row)));
      setDetail((row) =>
        row && row.id === threadId
          ? { ...row, unread: true, messages: row.messages.map((message) => ({ ...message, unread: true })) }
          : row
      );
    } catch (error) {
      setProblem(error.message);
    }
  }

  async function moveThread(destId) {
    if (!destId) return;
    setProblem('');
    try {
      await getJson(
        '/api/threads/' + encodeURIComponent(threadId) + '/move/' + encodeURIComponent(destId),
        { method: 'POST' }
      );
      await reload(mailboxId);
    } catch (error) {
      setProblem(error.message);
    }
  }

  async function signOut() {
    setProblem('');
    try {
      const status = await getJson('/api/sign-out', { method: 'POST' });
      setMode(status.mode || 'demo');
      setEmail('');
      setAccountOpen(false);
      setSignIn({ email: '', password: '', name: '', imapHost: '', imapPort: '993', smtpHost: '', smtpPort: '587' });
      await reload('inbox');
    } catch (error) {
      setProblem(error.message);
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

  const destinations = mailboxes.filter((mailbox) => mailbox.id !== mailboxId);
  const moveTarget = destinations.some((mailbox) => mailbox.id === moveTo) ? moveTo : destinations[0] ? destinations[0].id : '';
  const needle = query.trim();
  const shown = threads;
  const selected = shown.find((thread) => thread.id === threadId) || null;
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
          <button
            type="button"
            className={mailboxId === 'starred' ? 'box selected' : 'box'}
            onClick={() => openMailbox('starred')}
          >
            Starred
          </button>
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
              setSignIn((current) => (current.email ? current : { ...current, email }));
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
          <p className="empty">
            {needle ? 'Nothing matches.' : mailboxId === 'starred' ? 'Nothing starred.' : 'Nothing in this box.'}
          </p>
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
                  <span className="when">
                    {thread.starred ? <span className="flag">Starred</span> : null}
                    {thread.starred ? ' · ' : ''}
                    {whenLabel(thread.unixSecs)}
                  </span>
                </span>
                <span className="subject">{thread.subject}</span>
                <span className="preview">{thread.preview}</span>
              </span>
              {thread.unread ? <i className="mark" /> : null}
            </button>
          ))
        )}
        {mode === 'account' && mailboxId !== 'starred' ? (
          <button className="older" type="button" onClick={loadOlder} disabled={loadingOlder}>
            {loadingOlder ? 'Loading…' : 'Older'}
          </button>
        ) : null}
        {olderNote ? <p className="empty">{olderNote}</p> : null}
      </section>
      <article className="read">
        {problem ? <p className="problem">{problem}</p> : null}
        {accountOpen ? (
          <form className="form" onSubmit={signInSubmit}>
            <h1>{mode === 'account' ? 'Account' : 'Sign in'}</h1>
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
              {mode === 'account' ? (
                <button className="ghost" type="button" onClick={signOut}>
                  Sign out
                </button>
              ) : null}
            </div>
          </form>
        ) : compose ? (
          <form className="form" onSubmit={submit}>
            <h1>{draft.kind === 'forward' ? 'Forward' : draft.inReplyTo ? 'Reply' : 'New message'}</h1>
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
            <label>
              File
              <input
                type="file"
                onChange={(event) => {
                  const file = event.target.files && event.target.files[0];
                  if (!file) {
                    setDraft((current) => ({ ...current, fileName: '', fileMime: '', fileData: '' }));
                    return;
                  }
                  if (file.size > 200000) {
                    setProblem('That file is over 200 KB.');
                    event.target.value = '';
                    return;
                  }
                  const reader = new FileReader();
                  reader.onload = () => {
                    const text = String(reader.result || '');
                    const comma = text.indexOf(',');
                    setDraft((current) => ({
                      ...current,
                      fileName: file.name,
                      fileMime: file.type || 'application/octet-stream',
                      fileData: comma >= 0 ? text.slice(comma + 1) : '',
                    }));
                  };
                  reader.readAsDataURL(file);
                }}
              />
            </label>
            {draft.fileName ? <p className="note">{draft.fileName}</p> : null}
            <div className="actions">
              <button className="send" type="submit" disabled={sending}>
                {sending ? 'Sending…' : 'Send'}
              </button>
              <button className="ghost" type="button" onClick={saveDraft} disabled={sending}>
                Save draft
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
                    {message.html ? (
                      <iframe className="html-mail" sandbox="allow-popups" referrerPolicy="no-referrer" title={message.subject || 'Message'} srcDoc={message.html} />
                    ) : (
                      message.body.split('\n').map((line, index) => (
                        <p key={message.id + '-' + index}>{line || '\u00a0'}</p>
                      ))
                    )}
                  </div>
                  {message.attachments && message.attachments.length > 0 ? (
                    <ul className="files">
                      {message.attachments.map((file) => (
                        <li key={file.id}>
                          <a href={'/api/attachments/' + file.id}>{file.name}</a>
                          <span>{sizeLabel(file.size)}</span>
                        </li>
                      ))}
                    </ul>
                  ) : null}
                </div>
              ))
            ) : (
              <p className="empty">Loading…</p>
            )}
            {open ? (
              <div className="actions">
                <button type="button" onClick={() => replyTo(open)}>
                  Reply
                </button>
                <button className="ghost" type="button" onClick={() => forwardMessage(open)}>
                  Forward
                </button>
                <button className="ghost" type="button" onClick={() => changeThread('star')}>
                  {open.starred ? 'Starred' : 'Star'}
                </button>
                <button className="ghost" type="button" onClick={() => changeThread('unread')}>
                  Unread
                </button>
                {mailboxes.find((mailbox) => mailbox.id === mailboxId)?.role === 'archive' ? null : (
                  <button className="ghost" type="button" onClick={() => changeThread('archive')}>
                    Archive
                  </button>
                )}
                <button className="ghost" type="button" onClick={() => changeThread('trash')}>
                  Trash
                </button>
                {destinations.length > 0 ? (
                  <>
                    <select className="move" value={moveTarget} onChange={(event) => setMoveTo(event.target.value)}>
                      {destinations.map((mailbox) => (
                        <option key={mailbox.id} value={mailbox.id}>
                          {mailbox.name}
                        </option>
                      ))}
                    </select>
                    <button className="ghost" type="button" onClick={() => moveThread(moveTarget)}>
                      Move
                    </button>
                  </>
                ) : null}
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
