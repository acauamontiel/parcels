//! Types of `sync/maindata`, the incremental state endpoint.
//!
//! The server only sends what changed since the response id (`rid`) given in
//! the request. Every field is therefore optional: a missing field means
//! "unchanged", unless `full_update` is true, in which case the response is a
//! complete snapshot.

use std::collections::HashMap;

use serde::Deserialize;

use crate::TorrentState;

/// A `sync/maindata` response.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MainData {
    /// Response id, to send back in the next request
    pub rid: i64,
    /// The response is a full snapshot, not a diff
    pub full_update: bool,
    /// Changed torrents by hash, with only the changed fields
    pub torrents: HashMap<String, TorrentPatch>,
    pub torrents_removed: Vec<String>,
    /// Added or changed categories by name
    pub categories: HashMap<String, CategoryPatch>,
    pub categories_removed: Vec<String>,
    pub tags: Vec<String>,
    pub tags_removed: Vec<String>,
    pub server_state: Option<ServerStatePatch>,
}

/// Changed fields of a torrent. See `torrents/info` in the WebUI API docs.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TorrentPatch {
    pub name: Option<String>,
    pub state: Option<TorrentState>,
    /// From 0 to 1
    pub progress: Option<f64>,
    /// Bytes per second
    pub dlspeed: Option<i64>,
    pub upspeed: Option<i64>,
    /// Seconds; [`ETA_INFINITY`] when unknown
    pub eta: Option<i64>,
    /// Size of the selected files, in bytes
    pub size: Option<i64>,
    /// Size of all files; -1 before the metadata is known
    pub total_size: Option<i64>,
    pub downloaded: Option<i64>,
    pub uploaded: Option<i64>,
    pub amount_left: Option<i64>,
    pub ratio: Option<f64>,
    pub category: Option<String>,
    /// Comma-separated, e.g. `"iso, weekend"`
    pub tags: Option<String>,
    /// Unix time
    pub added_on: Option<i64>,
    /// Unix time; -1 until completed
    pub completion_on: Option<i64>,
    pub save_path: Option<String>,
    pub content_path: Option<String>,
    pub num_seeds: Option<i64>,
    pub num_complete: Option<i64>,
    pub num_leechs: Option<i64>,
    pub num_incomplete: Option<i64>,
    pub priority: Option<i64>,
    pub tracker: Option<String>,
    pub has_metadata: Option<bool>,
    /// Why the torrent is in the error state (WebAPI 2.17+)
    pub error: Option<String>,
}

/// Changed fields of a category.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct CategoryPatch {
    pub name: Option<String>,
    #[serde(rename = "savePath")]
    pub save_path: Option<String>,
}

/// Changed fields of the global transfer state.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ServerStatePatch {
    pub dl_info_speed: Option<i64>,
    pub up_info_speed: Option<i64>,
    pub dl_rate_limit: Option<i64>,
    pub up_rate_limit: Option<i64>,
    pub use_alt_speed_limits: Option<bool>,
    pub free_space_on_disk: Option<i64>,
    /// `connected`, `firewalled` or `disconnected`
    pub connection_status: Option<String>,
}

/// `eta` value qBittorrent uses for "unknown/infinite".
pub const ETA_INFINITY: i64 = 8_640_000;

/// Splits the `tags` field of a torrent.
pub fn split_tags(tags: &str) -> impl Iterator<Item = &str> {
    tags.split(',').map(str::trim).filter(|tag| !tag.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_tags() {
        assert_eq!(
            split_tags("iso, weekend").collect::<Vec<_>>(),
            ["iso", "weekend"]
        );
        assert_eq!(split_tags("").count(), 0);
        assert_eq!(split_tags(" a ,,b").collect::<Vec<_>>(), ["a", "b"]);
    }
}
