use crate::Version;

/// What the server reported when connecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    /// qBittorrent version, e.g. `5.1.2`
    pub version: Version,
    /// WebUI API version, e.g. `2.11.4`. Use it to gate newer endpoints.
    pub api_version: Version,
}
