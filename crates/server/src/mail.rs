//! Password-reset email delivery (#160; docs/server.md, Accounts and sign-in).
//!
//! Delivery is configured from `SMTP_URL` (`smtp://user:pass@host:port` or `smtps://...`) and
//! `MAIL_FROM`. With either unset, [`Mailer::disabled`] sends nothing and reports an error the
//! caller logs; the reset endpoint's answer does not change. The provider choice is still #110,
//! but the URL is generic, so a local mock and any SMTP provider both work.
//!
//! `lettre` is blocking, so callers run [`Mailer::send_reset_code`] on a blocking worker. The
//! `native-tls` feature verifies against the OS trust store, matching the healthchecks client
//! and keeping the bundled-CA (CDLA) crate out of the dependency graph. The runtime image
//! already installs `ca-certificates` for that reason.

use lettre::Transport;
use lettre::message::{Mailbox, Message};
use lettre::transport::smtp::SmtpTransport;

/// An SMTP sender, or a disabled placeholder when the environment does not configure one.
pub struct Mailer {
    transport: Option<SmtpTransport>,
    from: Option<Mailbox>,
}

impl Mailer {
    /// Sends nothing; [`Self::send_reset_code`] returns an error the caller logs.
    pub fn disabled() -> Self {
        Self {
            transport: None,
            from: None,
        }
    }

    /// Reads `SMTP_URL` and `MAIL_FROM`. An invalid value disables delivery with a `WARN` instead
    /// of failing startup: the rest of the server must keep running and the reset answer stays
    /// identical.
    pub fn from_env() -> Self {
        match (std::env::var("SMTP_URL"), std::env::var("MAIL_FROM")) {
            (Ok(url), Ok(from)) => match Self::from_url(&url, &from) {
                Ok(mailer) => mailer,
                Err(error) => {
                    tracing::warn!(%error, "email delivery disabled: invalid SMTP_URL/MAIL_FROM");
                    Self::disabled()
                }
            },
            _ => Self::disabled(),
        }
    }

    /// Builds a mailer from a URL and sender address, e.g. `smtp://127.0.0.1:2525` and
    /// `noreply@example.com`. Public so the tests can point it at a local mock SMTP listener.
    pub fn from_url(url: &str, from: &str) -> Result<Self, String> {
        let transport = SmtpTransport::from_url(url)
            .map_err(|error| error.to_string())?
            .build();
        let from = from.parse::<Mailbox>().map_err(|error| error.to_string())?;
        Ok(Self {
            transport: Some(transport),
            from: Some(from),
        })
    }

    pub fn is_enabled(&self) -> bool {
        self.transport.is_some()
    }

    /// Sends the six-digit code. Blocking: call it on `spawn_blocking`.
    pub fn send_reset_code(&self, to: &str, code: &str) -> Result<(), String> {
        let (Some(transport), Some(from)) = (&self.transport, &self.from) else {
            return Err("email delivery is not configured".to_owned());
        };
        let to = to.parse::<Mailbox>().map_err(|error| error.to_string())?;
        let message = Message::builder()
            .from(from.clone())
            .to(to)
            .subject("IWBTG Collection password reset")
            .body(format!(
                "Your password reset code is {code}.\n\n\
                 It expires in 15 minutes. If you did not ask to reset your password, ignore \
                 this message.\n"
            ))
            .map_err(|error| error.to_string())?;
        transport.send(&message).map_err(|error| error.to_string())?;
        Ok(())
    }
}
