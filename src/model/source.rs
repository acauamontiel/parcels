use gettextrs::gettext;
use qbittorrent_client::{MagnetLink, TorrentFileInfo, TorrentFileUpload, is_torrent_url};

use crate::utils;

/// Largest `.torrent` file Parcels reads; real ones are far smaller.
pub const MAX_TORRENT_FILE_SIZE: usize = 20 * 1024 * 1024;

/// Something to add: a magnet link or URL, or the contents of a `.torrent` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TorrentSource {
    Url(String),
    File(TorrentFileUpload),
}

impl TorrentSource {
    /// Extracts magnet links and URLs from text, one per line.
    pub fn from_text(text: &str) -> Vec<Self> {
        text.lines()
            .map(str::trim)
            .filter(|line| is_torrent_url(line))
            .map(|line| Self::Url(line.to_owned()))
            .collect()
    }

    /// A short name for lists.
    pub fn title(&self) -> String {
        match self {
            Self::Url(url) => MagnetLink::parse(url)
                .and_then(|magnet| magnet.display_name)
                .unwrap_or_else(|| url.clone()),
            Self::File(file) => TorrentFileInfo::parse(&file.data)
                .map(|info| info.name)
                .unwrap_or_else(|| file.file_name.clone()),
        }
    }

    /// What kind of source it is, and its size when known.
    pub fn subtitle(&self) -> String {
        match self {
            Self::Url(url) if MagnetLink::parse(url).is_some() => gettext("Magnet link"),
            Self::Url(_) => gettext("Link to a torrent file"),
            Self::File(file) => match TorrentFileInfo::parse(&file.data) {
                Some(info) if info.file_count > 1 => {
                    // Translators: {size} is e.g. "4.2 GB", {count} a number of files
                    gettext("{size} in {count} files")
                        .replace("{size}", &utils::format_size(info.total_size))
                        .replace("{count}", &info.file_count.to_string())
                }
                Some(info) => utils::format_size(info.total_size),
                None => gettext("Not a valid torrent file"),
            },
        }
    }

    /// Files that don't parse are rejected before upload, with a clear message.
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Url(_) => true,
            Self::File(file) => TorrentFileInfo::parse(&file.data).is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_urls_from_text() {
        let text = "magnet:?xt=urn:btih:abc&dn=One\n\nnot a link\n https://example.com/b.torrent ";
        let sources = TorrentSource::from_text(text);
        assert_eq!(
            sources,
            [
                TorrentSource::Url("magnet:?xt=urn:btih:abc&dn=One".into()),
                TorrentSource::Url("https://example.com/b.torrent".into()),
            ]
        );
        assert_eq!(sources[0].title(), "One");
        assert!(TorrentSource::from_text("hello").is_empty());
    }

    #[test]
    fn describes_files() {
        let file = TorrentSource::File(TorrentFileUpload {
            file_name: "x.torrent".into(),
            data: b"d4:infod6:lengthi2048e4:name8:file.isoee".to_vec(),
        });
        assert_eq!(file.title(), "file.iso");
        assert!(file.is_valid());

        let broken = TorrentSource::File(TorrentFileUpload {
            file_name: "broken.torrent".into(),
            data: b"garbage".to_vec(),
        });
        assert_eq!(broken.title(), "broken.torrent");
        assert!(!broken.is_valid());
    }
}
