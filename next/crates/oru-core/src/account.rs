//! A mailbox sign-in. The password stays in the local account file, not in the API.

use crate::json::{json_string, parse_flat};

#[derive(Clone)]
pub struct Account {
    pub name: String,
    pub email: String,
    pub password: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub smtp_host: String,
    pub smtp_port: u16,
}

impl std::fmt::Debug for Account {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Account")
            .field("email", &self.email)
            .field("imap_host", &self.imap_host)
            .field("smtp_host", &self.smtp_host)
            .finish()
    }
}

pub fn parse_account(input: &str) -> Result<Account, &'static str> {
    let pairs = parse_flat(input)?;
    let email = field(&pairs, "email").trim().to_string();
    let password = field(&pairs, "password");
    if !email.contains('@') || email.contains(char::is_whitespace) || password.is_empty() {
        return Err("email and password are required");
    }
    let (guess_imap, guess_imap_port, guess_smtp, guess_smtp_port) = guess_hosts(&email);
    let imap_host = nonempty(field(&pairs, "imapHost"), &guess_imap);
    let smtp_host = nonempty(field(&pairs, "smtpHost"), &guess_smtp);
    let name = nonempty(field(&pairs, "name"), email.split('@').next().unwrap_or("You"));
    Ok(Account {
        name,
        email,
        password,
        imap_host,
        imap_port: port(field(&pairs, "imapPort"), guess_imap_port)?,
        smtp_host,
        smtp_port: port(field(&pairs, "smtpPort"), guess_smtp_port)?,
    })
}

pub fn account_json(account: &Account) -> String {
    format!(
        "{{\"name\":{},\"email\":{},\"password\":{},\"imapHost\":{},\"imapPort\":\"{}\",\"smtpHost\":{},\"smtpPort\":\"{}\"}}",
        json_string(&account.name),
        json_string(&account.email),
        json_string(&account.password),
        json_string(&account.imap_host),
        account.imap_port,
        json_string(&account.smtp_host),
        account.smtp_port,
    )
}

/// Hosts for the big providers. Anything else uses imap. and smtp. on the domain.
pub fn guess_hosts(email: &str) -> (String, u16, String, u16) {
    let domain = email.rsplit('@').next().unwrap_or("").trim().to_ascii_lowercase();
    let (imap, smtp) = match domain.as_str() {
        "gmail.com" | "googlemail.com" => ("imap.gmail.com", "smtp.gmail.com"),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => ("outlook.office365.com", "smtp.office365.com"),
        "yahoo.com" => ("imap.mail.yahoo.com", "smtp.mail.yahoo.com"),
        "icloud.com" | "me.com" | "mac.com" => ("imap.mail.me.com", "smtp.mail.me.com"),
        other => return (format!("imap.{other}"), 993, format!("smtp.{other}"), 587),
    };
    (imap.into(), 993, smtp.into(), 587)
}

fn field(pairs: &[(String, String)], key: &str) -> String {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.clone())
        .unwrap_or_default()
}

fn nonempty(value: String, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() { fallback.to_string() } else { trimmed.to_string() }
}

fn port(value: String, fallback: u16) -> Result<u16, &'static str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(fallback);
    }
    trimmed.parse().map_err(|_| "bad port")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guesses_gmail_and_keeps_a_custom_host() {
        let (imap, port, smtp, smtp_port) = guess_hosts("Ada@Gmail.com");
        assert_eq!(imap, "imap.gmail.com");
        assert_eq!(port, 993);
        assert_eq!(smtp, "smtp.gmail.com");
        assert_eq!(smtp_port, 587);
        let account = parse_account(
            r#"{"email":"ada@studio.test","password":"secret","imapPort":143}"#,
        )
        .unwrap();
        assert_eq!(account.imap_host, "imap.studio.test");
        assert_eq!(account.imap_port, 143);
        assert_eq!(account.smtp_port, 587);
        assert!(!format!("{account:?}").contains("secret"));
    }
}
