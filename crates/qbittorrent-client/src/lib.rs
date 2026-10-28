//! Async client for the qBittorrent WebUI API.
//!
//! This crate has no GTK dependency. It needs a Tokio runtime, because it is
//! built on `reqwest`.
//!
//! Supported servers: qBittorrent 5.0 and later. API key authentication
//! requires qBittorrent 5.2 (WebAPI 2.14.1) or later.

mod add;
mod client;
mod error;
mod magnet;
pub mod maindata;
mod state;
mod torrent_file;
mod types;
mod version;

pub use add::{AddTorrents, ContentLayout, TorrentFileUpload};
pub use client::{Client, ClientConfig, Credentials, normalize_url};
pub use error::{Error, Result};
pub use magnet::{MagnetLink, is_torrent_url};
pub use maindata::MainData;
pub use state::TorrentState;
pub use torrent_file::TorrentFileInfo;
pub use types::ServerInfo;
pub use version::Version;

/// Oldest qBittorrent release Parcels talks to: 5.0 renamed the pause/resume
/// endpoints to stop/start and the `paused` parameter to `stopped`.
pub const MIN_SERVER_VERSION: Version = Version::new(5, 0, 0);
