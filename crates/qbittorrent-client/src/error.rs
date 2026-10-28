use reqwest::StatusCode;

use crate::Version;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors returned by [`crate::Client`].
///
/// The variants are coarse on purpose: each maps to a message and a recovery
/// action in the UI.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid server address: {0}")]
    InvalidUrl(String),

    /// The server could not be reached: DNS, refused connection, timeout…
    #[error("could not reach the server: {0}")]
    Network(#[source] reqwest::Error),

    /// The TLS handshake failed, usually because of a self-signed certificate.
    #[error("the server certificate is not trusted: {0}")]
    Certificate(#[source] reqwest::Error),

    /// Wrong username or password.
    #[error("invalid username or password")]
    InvalidCredentials,

    /// Too many failed login attempts; qBittorrent bans the IP for a while.
    #[error("this device is temporarily banned after too many failed login attempts")]
    Banned,

    /// The request was rejected after authenticating, or the API key is invalid.
    #[error("access denied")]
    Unauthorized,

    /// The address answers, but there is no qBittorrent WebUI API there.
    #[error("no qBittorrent WebUI found at this address")]
    NotQbittorrent,

    #[error("qBittorrent {found} is not supported, version {minimum} or later is required")]
    UnsupportedVersion { found: Version, minimum: Version },

    #[error("unexpected response from the server (HTTP {status})")]
    Http { status: StatusCode, body: String },

    #[error("could not parse the server response: {0}")]
    Parse(String),
}

impl Error {
    /// Whether retrying the same request later may succeed.
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Network(_) => true,
            Self::Http { status, .. } => status.is_server_error(),
            _ => false,
        }
    }

    pub(crate) fn from_reqwest(err: reqwest::Error) -> Self {
        if is_certificate_error(&err) {
            Self::Certificate(err)
        } else if err.is_decode() {
            Self::Parse(err.to_string())
        } else {
            Self::Network(err)
        }
    }
}

/// rustls doesn't surface a typed error through reqwest, so walk the source
/// chain looking for certificate validation failures.
fn is_certificate_error(err: &reqwest::Error) -> bool {
    let mut source: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = source {
        let text = e.to_string().to_ascii_lowercase();
        if text.contains("certificate") || text.contains("unknownissuer") {
            return true;
        }
        source = e.source();
    }
    false
}
