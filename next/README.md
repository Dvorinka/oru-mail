# Oru next

The rewrite. The Electron app in `../app` stays until this shell replaces it.

| Piece | Role |
|---|---|
| `crates/oru-core` | Mail model and `MailStore`. No network yet. |
| `crates/oru-cli` | `oru` prints the demo mailbox. Proves the core links. |
| `ui` | React shell. Web build, and the same layout under 800px for phones. |

Desktop on Windows and Linux will be a Tauri window around `ui`, calling `oru-core` for the mailbox. Phone builds use the same UI. IMAP and SMTP land in the core behind `MailStore`, not in the React tree.

```
cargo test
cargo run -p oru-cli
cd ui && npm install && npm run dev
```

The toolchain shims in `~/.cargo/bin` are empty links on this machine. Call rustc and cargo from `~/.rustup/toolchains/stable-x86_64-pc-windows-msvc/bin`.
