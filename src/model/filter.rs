use gettextrs::gettext;

use super::Torrent;

/// What the sidebar shows: a status, a category or a tag.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum Filter {
    #[default]
    All,
    Downloading,
    Seeding,
    Completed,
    Stopped,
    /// Transferring data right now
    Active,
    Errored,
    /// A category; the empty string means "no category"
    Category(String),
    /// A tag; the empty string means "no tags"
    Tag(String),
}

impl Filter {
    pub const STATUSES: [Self; 7] = [
        Self::All,
        Self::Downloading,
        Self::Seeding,
        Self::Completed,
        Self::Stopped,
        Self::Active,
        Self::Errored,
    ];

    pub fn matches(&self, torrent: &Torrent) -> bool {
        let state = torrent.state();
        match self {
            Self::All => true,
            Self::Downloading => state.is_downloading() && !state.is_stopped(),
            Self::Seeding => state.is_seeding(),
            Self::Completed => state.is_completed(),
            Self::Stopped => state.is_stopped(),
            Self::Active => torrent.download_speed() > 0 || torrent.upload_speed() > 0,
            Self::Errored => state.is_errored(),
            Self::Category(category) => torrent.category() == *category,
            Self::Tag(tag) if tag.is_empty() => torrent.tag_list().is_empty(),
            Self::Tag(tag) => torrent.has_tag(tag),
        }
    }

    pub fn title(&self) -> String {
        match self {
            Self::All => gettext("All"),
            Self::Downloading => gettext("Downloading"),
            Self::Seeding => gettext("Seeding"),
            Self::Completed => gettext("Completed"),
            Self::Stopped => gettext("Stopped"),
            Self::Active => gettext("Active"),
            Self::Errored => gettext("Errors"),
            Self::Category(c) if c.is_empty() => gettext("No Category"),
            Self::Tag(t) if t.is_empty() => gettext("No Tags"),
            Self::Category(name) | Self::Tag(name) => name.clone(),
        }
    }

    pub fn icon_name(&self) -> &'static str {
        match self {
            Self::All => "view-list-symbolic",
            Self::Downloading => "folder-download-symbolic",
            Self::Seeding => "network-transmit-symbolic",
            Self::Completed => "object-select-symbolic",
            Self::Stopped => "media-playback-pause-symbolic",
            Self::Active => "network-transmit-receive-symbolic",
            Self::Errored => "dialog-error-symbolic",
            Self::Category(_) => "folder-symbolic",
            Self::Tag(_) => "tag-symbolic",
        }
    }
}

/// Case-insensitive match of every word of `query` in the torrent name.
pub fn matches_search(torrent: &Torrent, query: &str) -> bool {
    let name = torrent.name().to_lowercase();
    query
        .split_whitespace()
        .all(|word| name.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use qbittorrent_client::TorrentState;
    use qbittorrent_client::maindata::TorrentPatch;

    use super::*;

    fn torrent(state: TorrentState, category: &str, tags: &str) -> Torrent {
        let torrent = Torrent::new("hash");
        torrent.apply(&TorrentPatch {
            name: Some("Debian 13 DVD".into()),
            state: Some(state),
            category: Some(category.into()),
            tags: Some(tags.into()),
            ..Default::default()
        });
        torrent
    }

    #[test]
    fn status_filters() {
        let stopped_download = torrent(TorrentState::StoppedDownloading, "", "");
        assert!(Filter::Stopped.matches(&stopped_download));
        assert!(!Filter::Downloading.matches(&stopped_download));
        assert!(!Filter::Completed.matches(&stopped_download));

        let seeding = torrent(TorrentState::StalledUploading, "", "");
        assert!(Filter::Seeding.matches(&seeding));
        assert!(Filter::Completed.matches(&seeding));
        assert!(!Filter::Active.matches(&seeding));

        assert!(Filter::Errored.matches(&torrent(TorrentState::MissingFiles, "", "")));
    }

    #[test]
    fn label_filters() {
        let t = torrent(TorrentState::Downloading, "linux", "iso, weekend");
        assert!(Filter::Category("linux".into()).matches(&t));
        assert!(!Filter::Category(String::new()).matches(&t));
        assert!(Filter::Tag("weekend".into()).matches(&t));
        assert!(!Filter::Tag("week".into()).matches(&t));
        assert!(!Filter::Tag(String::new()).matches(&t));
        assert!(Filter::Tag(String::new()).matches(&torrent(TorrentState::Downloading, "", "")));
    }

    #[test]
    fn search() {
        let t = torrent(TorrentState::Downloading, "", "");
        assert!(matches_search(&t, "debian dvd"));
        assert!(matches_search(&t, "  "));
        assert!(!matches_search(&t, "debian cd"));
    }
}
