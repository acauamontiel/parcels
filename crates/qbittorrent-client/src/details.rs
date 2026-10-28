//! Per-torrent details: properties, files, trackers and peers.

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

/// `torrents/properties`. Sizes in bytes, times in seconds, dates as Unix
/// time with -1 meaning "never/unknown".
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TorrentProperties {
    pub save_path: String,
    pub download_path: Option<String>,
    pub creation_date: i64,
    pub created_by: String,
    pub comment: String,
    pub addition_date: i64,
    pub completion_date: i64,
    pub last_seen: i64,
    pub piece_size: i64,
    pub pieces_num: i64,
    pub pieces_have: i64,
    pub total_size: i64,
    pub total_downloaded: i64,
    pub total_uploaded: i64,
    pub total_wasted: i64,
    pub share_ratio: f64,
    pub dl_speed: i64,
    pub up_speed: i64,
    pub dl_speed_avg: i64,
    pub up_speed_avg: i64,
    /// -1 when unlimited
    pub dl_limit: i64,
    pub up_limit: i64,
    pub eta: i64,
    pub time_elapsed: i64,
    pub seeding_time: i64,
    pub nb_connections: i64,
    pub nb_connections_limit: i64,
    pub seeds: i64,
    pub seeds_total: i64,
    pub peers: i64,
    pub peers_total: i64,
    pub availability: f64,
    pub reannounce: i64,
    #[serde(alias = "isPrivate")]
    pub is_private: bool,
    pub infohash_v1: String,
    pub infohash_v2: String,
    /// Absent before WebAPI 2.10
    pub has_metadata: Option<bool>,
}

/// Download priority of a file, as `torrents/filePrio` takes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilePriority {
    Skip,
    Normal,
    High,
    Maximum,
}

impl FilePriority {
    pub const ALL: [Self; 4] = [Self::Skip, Self::Normal, Self::High, Self::Maximum];

    pub fn value(self) -> i64 {
        match self {
            Self::Skip => 0,
            Self::Normal => 1,
            Self::High => 6,
            Self::Maximum => 7,
        }
    }

    /// Older clients used the whole 1–7 range; map it onto the four levels.
    pub fn from_value(value: i64) -> Self {
        match value {
            ..=0 => Self::Skip,
            1..=5 => Self::Normal,
            6 => Self::High,
            _ => Self::Maximum,
        }
    }
}

/// An entry of `torrents/files`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct TorrentFile {
    /// File index, used by `torrents/filePrio`
    pub index: u32,
    /// Path inside the torrent, `/`-separated
    pub name: String,
    pub size: i64,
    pub progress: f64,
    priority: i64,
    pub availability: f64,
}

impl TorrentFile {
    pub fn priority(&self) -> FilePriority {
        FilePriority::from_value(self.priority)
    }
}

/// Status of a tracker in `torrents/trackers`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackerStatus {
    Disabled,
    NotContacted,
    Working,
    Updating,
    NotWorking,
    /// WebAPI 2.13+
    TrackerError,
    /// WebAPI 2.13+
    Unreachable,
}

impl TrackerStatus {
    fn from_value(value: i64) -> Self {
        match value {
            0 => Self::Disabled,
            2 => Self::Working,
            3 => Self::Updating,
            4 => Self::NotWorking,
            5 => Self::TrackerError,
            6 => Self::Unreachable,
            _ => Self::NotContacted,
        }
    }
}

/// An entry of `torrents/trackers`. DHT, PeX and LSD appear as pseudo
/// trackers with URLs like `** [DHT] **`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Tracker {
    pub url: String,
    status: i64,
    /// -1 for DHT, PeX and LSD (sent as an empty string by older versions)
    #[serde(deserialize_with = "tier")]
    pub tier: i64,
    /// -1 when unknown
    pub num_peers: i64,
    pub num_seeds: i64,
    pub num_leeches: i64,
    pub num_downloaded: i64,
    pub msg: String,
}

impl Tracker {
    pub fn status(&self) -> TrackerStatus {
        TrackerStatus::from_value(self.status)
    }

    /// `DHT`, `PeX` or `LSD` for the pseudo trackers.
    pub fn pseudo_source(&self) -> Option<&str> {
        self.url
            .strip_prefix("** [")
            .and_then(|rest| rest.strip_suffix("] **"))
    }
}

fn tier<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Tier {
        Number(i64),
        Text(String),
    }
    Ok(match Tier::deserialize(deserializer)? {
        Tier::Number(tier) => tier,
        Tier::Text(text) => text.parse().unwrap_or(-1),
    })
}

/// A `sync/torrentPeers` response: incremental, like `sync/maindata`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PeersData {
    pub rid: i64,
    pub full_update: bool,
    /// Changed peers by `ip:port`, with only the changed fields
    pub peers: HashMap<String, PeerPatch>,
    pub peers_removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct PeerPatch {
    pub ip: Option<String>,
    pub port: Option<u16>,
    /// Set when the server resolves peer host names
    pub host_name: Option<String>,
    /// For I2P peers, which have no IP and port
    pub i2p_dest: Option<String>,
    pub client: Option<String>,
    /// e.g. `BT`, `μTP`
    pub connection: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub progress: Option<f64>,
    pub dl_speed: Option<i64>,
    pub up_speed: Option<i64>,
    pub downloaded: Option<i64>,
    pub uploaded: Option<i64>,
    pub flags: Option<String>,
    pub flags_desc: Option<String>,
    /// Files the peer is exchanging, newline-separated
    pub files: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priorities() {
        for priority in FilePriority::ALL {
            assert_eq!(FilePriority::from_value(priority.value()), priority);
        }
        assert_eq!(FilePriority::from_value(4), FilePriority::Normal);
    }

    #[test]
    fn tiers_as_numbers_or_strings() {
        let parse = |json| serde_json::from_str::<Tracker>(json).unwrap().tier;
        assert_eq!(parse(r#"{"tier": 2}"#), 2);
        assert_eq!(parse(r#"{"tier": ""}"#), -1);
        assert_eq!(parse(r#"{"tier": -1}"#), -1);
    }
}
