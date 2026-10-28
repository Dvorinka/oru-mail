# Oru next

The rewrite. The Electron app in `../app` stays until this shell replaces it.

| Piece | Role |
|---|---|
| `crates/oru-core` | Mail model, `MailStore`, and the JSON the shell reads. No network stack. |
| `crates/oru-cli` | `oru` lists the demo mailbox, `oru serve` hosts the API and the UI, `oru desktop` opens a window. |
| `ui` | React shell. One page for the web, the desktop window, and phones. |

Web is `oru serve`. Phones use that same page: under 800px the message stacks, and `manifest.webmanifest` makes it installable. Desktop on Windows and Linux is `oru desktop`, which opens Edge or Chrome in app mode against the local server. The window uses a private browser profile so it stays up until the user closes it.

IMAP and SMTP still sit behind `MailStore`. The server today is `MemoryStore::demo` plus messages you send in the window. Those sends live in that process only.

The toolchain shims in `~/.cargo/bin` are empty links on this machine. Call rustc and cargo from `~/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin`.

```
cargo test
cargo run -p oru-cli
cd ui && npm install && npm run build
cargo run -p oru-cli -- serve --ui ui/dist
cargo run -p oru-cli -- desktop --ui ui/dist
```

`npm run dev` in `ui` proxies `/api` to `127.0.0.1:8787`, so `oru serve` has to be running. `--host 0.0.0.0` binds every interface when a phone on the same network should open the page. The default bind is loopback.
