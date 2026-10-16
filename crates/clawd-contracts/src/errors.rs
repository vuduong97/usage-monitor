//! Error types shared across crates. Display strings reproduce the Python messages, because
//! the settings window shows (and `settings_api._err` matches on) them.

use thiserror::Error;

/// `clockapi.AuthNeeded`: the clock has a web password and ours is missing or wrong.
/// A `ClockError` subclass in Python; here it converts into [`ClockError::AuthNeeded`].
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the clock wants its web password")]
pub struct AuthNeeded;

/// `clockapi.ClockError`: the clock did not answer, or answered with an error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClockError {
    /// HTTP 401 (`AuthNeeded` in Python).
    #[error("the clock wants its web password")]
    AuthNeeded,
    /// HTTP >= 400: `HTTP <status>: <first 200 chars of the body>`.
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },
    /// Connect error or timeout after the retries: `no answer from <base> (<error kind>)`.
    /// `settings_api._err` tells this one apart by the `no answer` prefix.
    #[error("no answer from {base} ({kind})")]
    NoAnswer { base: String, kind: String },
    /// Every other `ClockError(<text>)` of the Python client (`odd /api/heap answer`,
    /// `not JSON from <path>`, `save refused: ...`, `firmware size ...`, a malformed host).
    #[error("{0}")]
    Other(String),
}

impl From<AuthNeeded> for ClockError {
    fn from(_: AuthNeeded) -> Self {
        ClockError::AuthNeeded
    }
}

/// `feed.FeedError`: the feed did not answer, refused, or sent something that does not check out.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct FeedError(pub String);

/// `appupdate.UpdateError`. Messages stay Vietnamese as in Python; new minisign failures are
/// new Vietnamese texts recorded in the phase-10 fixtures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct UpdateError(pub String);

/// `usage.TokenExpired`: a 401/403 from the usage API; only the token's owner can refresh it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("the access token expired or was refused")]
pub struct TokenExpired;

/// `usage.UsageNotApplicable`: the usage endpoint has no 5h/7d windows for this account.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct UsageNotApplicable(pub String);

/// `agents.base.AgentError`: a config we must not touch (it does not parse, or a file that is
/// not ours is in the way). The text is shown to the user (Vietnamese).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct AgentError(pub String);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_python() {
        assert_eq!(
            ClockError::NoAnswer {
                base: "http://10.0.0.5".into(),
                kind: "ConnectError".into()
            }
            .to_string(),
            "no answer from http://10.0.0.5 (ConnectError)"
        );
        assert_eq!(
            ClockError::Http {
                status: 500,
                body: "boom".into()
            }
            .to_string(),
            "HTTP 500: boom"
        );
        assert_eq!(ClockError::from(AuthNeeded), ClockError::AuthNeeded);
    }
}
