# Oru next

The rewrite. The Electron app in `../app` stays until this shell replaces it.

| Piece | Role |
|---|---|
| `crates/oru-core` | Mail model, JSON, and IMAP/SMTP over a byte stream. TLS stays in the CLI. |
| `crates/oru-cli` | `oru` lists the demo mailbox, `oru serve` hosts the API and the UI, `oru desktop` opens a window. |
| `ui` | React shell. One page for the web, the desktop window, and phones. |

Web is `oru serve`. Phones use that same page: under 800px the message stacks, and `manifest.webmanifest` makes it installable. Desktop on Windows and Linux is `oru desktop`, which opens Edge or Chrome in app mode against the local server. The window uses a private browser profile so it stays up until the user closes it.

With no account file, the server shows the sample mailbox. Search posts to the open mailbox. A signed-in account runs IMAP SEARCH there and returns the newest 40 matches; the next page is the same search below the oldest uid. The sample mailbox searches stored messages, including the body. Compose can attach one file, up to 200 KB. Downloading it stays `application/octet-stream`. Compose sends To, Cc, and Bcc. The message lists every recipient. Bcc is left off the copy other people receive. Sign in from the rail and Oru reads IMAP and sends through SMTP. The password is written to `%APPDATA%\Oru\account.json` on Windows and `~/.config/oru/account.json` on Linux. Gmail and Outlook need an app password. Each sync keeps the newest 40 messages in a mailbox. Older asks for the 40 before those. Port 993 and 465 are implicit TLS. Port 143 and 587 use STARTTLS. A message you send is also appended to the server's Sent mailbox. Reply sets In-Reply-To. Trash stores `\Deleted` and the next sync skips those messages. Archive copies the message into the Archive mailbox and then stores `\Deleted` on the source. HTML mail is sanitized and shown in a frame with scripts disabled. Files are downloaded as attachments and are not opened as pages. Unread clears `\Seen`. Star stores `\Flagged` on every message in the thread. Forward quotes the message and leaves the recipient empty. Move copies a thread into another mailbox and then stores `\Deleted` on the source. Save draft files the message in Drafts and appends it there on a signed-in account. A send still needs an address. Starred lists flagged threads from every mailbox already stored. Sign out deletes the account file and returns to the sample mailbox.

The toolchain shims in `~/.cargo/bin` are empty links on this machine. Call rustc and cargo from `~/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin`.

```
cargo test
cargo run -p oru-cli
cd ui && npm install && npm run build
cargo run -p oru-cli -- serve --ui ui/dist
cargo run -p oru-cli -- desktop --ui ui/dist
```

`npm run dev` in `ui` proxies `/api` to `127.0.0.1:8787`, so `oru serve` has to be running. `--host 0.0.0.0` binds every interface when a phone on the same network should open the page. The default bind is loopback.
