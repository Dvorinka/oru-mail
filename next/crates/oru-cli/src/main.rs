use oru_core::{MailStore, MemoryStore};
use server::Opts;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

mod net;
mod server;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None => {
            list();
            ExitCode::SUCCESS
        }
        Some("serve") => command(args, false),
        Some("desktop") => command(args, true),
        Some("help") | Some("--help") | Some("-h") => {
            help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("unknown command {other}");
            help();
            ExitCode::from(2)
        }
    }
}

fn command(args: impl Iterator<Item = String>, open_window: bool) -> ExitCode {
    let opts = match parse_opts(args) {
        Ok(opts) => opts,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    let result = if open_window { server::desktop(opts) } else { server::serve(opts) };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn parse_opts(args: impl Iterator<Item = String>) -> Result<Opts, String> {
    let mut opts = Opts {
        host: "127.0.0.1".into(),
        port: 8787,
        ui: None,
    };
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => {
                let value = args.next().ok_or("--port needs a number")?;
                opts.port = value.parse().map_err(|_| format!("bad port {value}"))?;
            }
            "--host" => {
                opts.host = args.next().ok_or("--host needs an address")?;
                if !opts.host.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == ':') {
                    return Err(format!("bad host {}", opts.host));
                }
            }
            "--ui" => {
                opts.ui = Some(PathBuf::from(args.next().ok_or("--ui needs a path")?));
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(opts)
}

fn list() {
    let store = MemoryStore::demo();
    for mailbox in store.mailboxes() {
        println!("{}", mailbox.name);
        for thread in store.threads(&mailbox.id) {
            let mark = if thread.unread() { "*" } else { " " };
            let from = thread
                .latest()
                .map(|message| message.from.name.clone())
                .unwrap_or_default();
            println!("  {mark} {from} — {}", thread.subject);
        }
    }
}

fn help() {
    eprintln!(
        "\
oru
oru serve [--host 127.0.0.1] [--port 8787] [--ui ui/dist]
oru desktop [--port 8787] [--ui ui/dist]

serve   API and web UI. Phones use the same page under 800px.
desktop A window on Windows or Linux (Edge or Chrome app mode).
        Sign-in is saved in the account file (APPDATA\\Oru or ~/.config/oru)."
    );
}
