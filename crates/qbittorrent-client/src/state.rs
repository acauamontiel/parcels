use serde::Deserialize;

/// The `state` of a torrent, as reported by qBittorrent 5.x.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum TorrentState {
    #[serde(rename = "error")]
    Error,
    #[serde(rename = "missingFiles")]
    MissingFiles,
    #[serde(rename = "uploading")]
    Uploading,
    #[serde(rename = "stoppedUP", alias = "pausedUP")]
    StoppedUploading,
    #[serde(rename = "queuedUP")]
    QueuedUploading,
    #[serde(rename = "stalledUP")]
    StalledUploading,
    #[serde(rename = "checkingUP")]
    CheckingUploading,
    #[serde(rename = "forcedUP")]
    ForcedUploading,
    #[serde(rename = "allocating")]
    Allocating,
    #[serde(rename = "downloading")]
    Downloading,
    #[serde(rename = "metaDL")]
    DownloadingMetadata,
    #[serde(rename = "forcedMetaDL")]
    ForcedDownloadingMetadata,
    #[serde(rename = "stoppedDL", alias = "pausedDL")]
    StoppedDownloading,
    #[serde(rename = "queuedDL")]
    QueuedDownloading,
    #[serde(rename = "stalledDL")]
    StalledDownloading,
    #[serde(rename = "checkingDL")]
    CheckingDownloading,
    #[serde(rename = "forcedDL")]
    ForcedDownloading,
    #[serde(rename = "checkingResumeData")]
    CheckingResumeData,
    #[serde(rename = "moving")]
    Moving,
    /// Any state this version of Parcels doesn't know about
    #[serde(other)]
    Unknown,
}

impl TorrentState {
    /// Still downloading, including stopped and queued downloads.
    pub fn is_downloading(self) -> bool {
        matches!(
            self,
            Self::Downloading
                | Self::DownloadingMetadata
                | Self::ForcedDownloadingMetadata
                | Self::StalledDownloading
                | Self::CheckingDownloading
                | Self::StoppedDownloading
                | Self::QueuedDownloading
                | Self::ForcedDownloading
                | Self::Allocating
        )
    }

    /// Complete and running (seeding, possibly idle or queued).
    pub fn is_seeding(self) -> bool {
        matches!(
            self,
            Self::Uploading
                | Self::StalledUploading
                | Self::CheckingUploading
                | Self::QueuedUploading
                | Self::ForcedUploading
        )
    }

    /// All the data is downloaded.
    pub fn is_completed(self) -> bool {
        self.is_seeding() || self == Self::StoppedUploading
    }

    pub fn is_stopped(self) -> bool {
        matches!(self, Self::StoppedDownloading | Self::StoppedUploading)
    }

    pub fn is_errored(self) -> bool {
        matches!(self, Self::Error | Self::MissingFiles)
    }

    pub fn is_checking(self) -> bool {
        matches!(
            self,
            Self::CheckingDownloading | Self::CheckingUploading | Self::CheckingResumeData
        )
    }

    pub fn is_queued(self) -> bool {
        matches!(self, Self::QueuedDownloading | Self::QueuedUploading)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> TorrentState {
        serde_json::from_value(serde_json::Value::String(s.into())).unwrap()
    }

    #[test]
    fn parses_states_and_legacy_names() {
        assert_eq!(parse("stoppedUP"), TorrentState::StoppedUploading);
        assert_eq!(parse("pausedUP"), TorrentState::StoppedUploading);
        assert_eq!(parse("metaDL"), TorrentState::DownloadingMetadata);
        assert_eq!(parse("somethingNew"), TorrentState::Unknown);
    }

    #[test]
    fn classifies_states() {
        assert!(TorrentState::StoppedDownloading.is_downloading());
        assert!(TorrentState::StoppedDownloading.is_stopped());
        assert!(!TorrentState::StoppedUploading.is_seeding());
        assert!(TorrentState::StoppedUploading.is_completed());
        assert!(TorrentState::MissingFiles.is_errored());
    }
}
