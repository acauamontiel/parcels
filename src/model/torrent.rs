use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::subclass::prelude::*;
use qbittorrent_client::TorrentState;
use qbittorrent_client::maindata::{ETA_INFINITY, TorrentPatch, split_tags};

/// [`TorrentState`], registered with GObject so it can be a property.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ParcelsTorrentStatus")]
pub enum TorrentStatus {
    Error,
    MissingFiles,
    Uploading,
    StoppedUploading,
    QueuedUploading,
    StalledUploading,
    CheckingUploading,
    ForcedUploading,
    Allocating,
    Downloading,
    DownloadingMetadata,
    ForcedDownloadingMetadata,
    StoppedDownloading,
    QueuedDownloading,
    StalledDownloading,
    CheckingDownloading,
    ForcedDownloading,
    CheckingResumeData,
    Moving,
    #[default]
    Unknown,
}

impl From<TorrentState> for TorrentStatus {
    fn from(state: TorrentState) -> Self {
        match state {
            TorrentState::Error => Self::Error,
            TorrentState::MissingFiles => Self::MissingFiles,
            TorrentState::Uploading => Self::Uploading,
            TorrentState::StoppedUploading => Self::StoppedUploading,
            TorrentState::QueuedUploading => Self::QueuedUploading,
            TorrentState::StalledUploading => Self::StalledUploading,
            TorrentState::CheckingUploading => Self::CheckingUploading,
            TorrentState::ForcedUploading => Self::ForcedUploading,
            TorrentState::Allocating => Self::Allocating,
            TorrentState::Downloading => Self::Downloading,
            TorrentState::DownloadingMetadata => Self::DownloadingMetadata,
            TorrentState::ForcedDownloadingMetadata => Self::ForcedDownloadingMetadata,
            TorrentState::StoppedDownloading => Self::StoppedDownloading,
            TorrentState::QueuedDownloading => Self::QueuedDownloading,
            TorrentState::StalledDownloading => Self::StalledDownloading,
            TorrentState::CheckingDownloading => Self::CheckingDownloading,
            TorrentState::ForcedDownloading => Self::ForcedDownloading,
            TorrentState::CheckingResumeData => Self::CheckingResumeData,
            TorrentState::Moving => Self::Moving,
            TorrentState::Unknown => Self::Unknown,
        }
    }
}

impl From<TorrentStatus> for TorrentState {
    fn from(status: TorrentStatus) -> Self {
        match status {
            TorrentStatus::Error => Self::Error,
            TorrentStatus::MissingFiles => Self::MissingFiles,
            TorrentStatus::Uploading => Self::Uploading,
            TorrentStatus::StoppedUploading => Self::StoppedUploading,
            TorrentStatus::QueuedUploading => Self::QueuedUploading,
            TorrentStatus::StalledUploading => Self::StalledUploading,
            TorrentStatus::CheckingUploading => Self::CheckingUploading,
            TorrentStatus::ForcedUploading => Self::ForcedUploading,
            TorrentStatus::Allocating => Self::Allocating,
            TorrentStatus::Downloading => Self::Downloading,
            TorrentStatus::DownloadingMetadata => Self::DownloadingMetadata,
            TorrentStatus::ForcedDownloadingMetadata => Self::ForcedDownloadingMetadata,
            TorrentStatus::StoppedDownloading => Self::StoppedDownloading,
            TorrentStatus::QueuedDownloading => Self::QueuedDownloading,
            TorrentStatus::StalledDownloading => Self::StalledDownloading,
            TorrentStatus::CheckingDownloading => Self::CheckingDownloading,
            TorrentStatus::ForcedDownloading => Self::ForcedDownloading,
            TorrentStatus::CheckingResumeData => Self::CheckingResumeData,
            TorrentStatus::Moving => Self::Moving,
            TorrentStatus::Unknown => Self::Unknown,
        }
    }
}

mod imp {
    use super::*;

    #[derive(Debug, glib::Properties)]
    #[properties(wrapper_type = super::Torrent)]
    pub struct Torrent {
        #[property(get, construct_only)]
        hash: RefCell<String>,
        #[property(get, set)]
        name: RefCell<String>,
        #[property(get, set, builder(TorrentStatus::default()))]
        status: Cell<TorrentStatus>,
        /// From 0 to 1
        #[property(get, set)]
        progress: Cell<f64>,
        /// Download speed in bytes per second
        #[property(get, set)]
        download_speed: Cell<i64>,
        #[property(get, set)]
        upload_speed: Cell<i64>,
        /// Seconds left, or -1 when unknown
        #[property(get, set)]
        eta: Cell<i64>,
        /// Size of the selected files
        #[property(get, set)]
        size: Cell<i64>,
        #[property(get, set)]
        total_size: Cell<i64>,
        #[property(get, set)]
        downloaded: Cell<i64>,
        #[property(get, set)]
        uploaded: Cell<i64>,
        #[property(get, set)]
        amount_left: Cell<i64>,
        #[property(get, set)]
        ratio: Cell<f64>,
        #[property(get, set)]
        category: RefCell<String>,
        /// Comma-separated, as sent by the server; see [`super::Torrent::tag_list`]
        #[property(get, set)]
        tags: RefCell<String>,
        /// Unix time
        #[property(get, set)]
        added_on: Cell<i64>,
        /// Unix time, -1 until completed
        #[property(get, set)]
        completion_on: Cell<i64>,
        #[property(get, set)]
        save_path: RefCell<String>,
        #[property(get, set)]
        content_path: RefCell<String>,
        #[property(get, set)]
        seeds: Cell<i64>,
        #[property(get, set)]
        seeds_total: Cell<i64>,
        #[property(get, set)]
        peers: Cell<i64>,
        #[property(get, set)]
        peers_total: Cell<i64>,
        /// Queue position, 0 when not queued
        #[property(get, set)]
        priority: Cell<i64>,
        #[property(get, set)]
        tracker: RefCell<String>,
        #[property(get, set)]
        has_metadata: Cell<bool>,
        /// Why the torrent is in the error state
        #[property(get, set)]
        error: RefCell<String>,
    }

    impl Default for Torrent {
        fn default() -> Self {
            Self {
                hash: Default::default(),
                name: Default::default(),
                status: Default::default(),
                progress: Default::default(),
                download_speed: Default::default(),
                upload_speed: Default::default(),
                eta: Cell::new(-1),
                size: Default::default(),
                total_size: Default::default(),
                downloaded: Default::default(),
                uploaded: Default::default(),
                amount_left: Default::default(),
                ratio: Default::default(),
                category: Default::default(),
                tags: Default::default(),
                added_on: Default::default(),
                completion_on: Cell::new(-1),
                save_path: Default::default(),
                content_path: Default::default(),
                seeds: Default::default(),
                seeds_total: Default::default(),
                peers: Default::default(),
                peers_total: Default::default(),
                priority: Default::default(),
                tracker: Default::default(),
                has_metadata: Default::default(),
                error: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Torrent {
        const NAME: &'static str = "ParcelsTorrent";
        type Type = super::Torrent;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Torrent {}
}

glib::wrapper! {
    /// A torrent on the server, kept up to date by [`super::TorrentList`].
    pub struct Torrent(ObjectSubclass<imp::Torrent>);
}

/// Sets a property only when the value changed, so `notify` fires only for
/// real changes and rows don't redraw for nothing.
macro_rules! update {
    ($obj:ident, $patch:ident, $field:ident => $getter:ident, $setter:ident) => {
        if let Some(value) = &$patch.$field {
            if $obj.$getter() != *value {
                $obj.$setter(value.clone());
            }
        }
    };
}

impl Torrent {
    pub fn new(hash: &str) -> Self {
        glib::Object::builder().property("hash", hash).build()
    }

    pub fn state(&self) -> TorrentState {
        self.status().into()
    }

    pub fn tag_list(&self) -> Vec<String> {
        split_tags(&self.tags()).map(str::to_owned).collect()
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        split_tags(&self.tags()).any(|t| t == tag)
    }

    /// Applies the fields present in `patch`; absent fields are unchanged.
    pub fn apply(&self, patch: &TorrentPatch) {
        // Emit all `notify` signals at the end, once per property
        let _guard = self.freeze_notify();

        update!(self, patch, name => name, set_name);
        if let Some(state) = patch.state {
            let status = TorrentStatus::from(state);
            if self.status() != status {
                self.set_status(status);
            }
        }
        update!(self, patch, progress => progress, set_progress);
        update!(self, patch, dlspeed => download_speed, set_download_speed);
        update!(self, patch, upspeed => upload_speed, set_upload_speed);
        if let Some(eta) = patch.eta {
            let eta = if eta >= ETA_INFINITY { -1 } else { eta };
            if self.eta() != eta {
                self.set_eta(eta);
            }
        }
        update!(self, patch, size => size, set_size);
        update!(self, patch, total_size => total_size, set_total_size);
        update!(self, patch, downloaded => downloaded, set_downloaded);
        update!(self, patch, uploaded => uploaded, set_uploaded);
        update!(self, patch, amount_left => amount_left, set_amount_left);
        update!(self, patch, ratio => ratio, set_ratio);
        update!(self, patch, category => category, set_category);
        update!(self, patch, tags => tags, set_tags);
        update!(self, patch, added_on => added_on, set_added_on);
        update!(self, patch, completion_on => completion_on, set_completion_on);
        update!(self, patch, save_path => save_path, set_save_path);
        update!(self, patch, content_path => content_path, set_content_path);
        update!(self, patch, num_seeds => seeds, set_seeds);
        update!(self, patch, num_complete => seeds_total, set_seeds_total);
        update!(self, patch, num_leechs => peers, set_peers);
        update!(self, patch, num_incomplete => peers_total, set_peers_total);
        update!(self, patch, priority => priority, set_priority);
        update!(self, patch, tracker => tracker, set_tracker);
        update!(self, patch, has_metadata => has_metadata, set_has_metadata);
        update!(self, patch, error => error, set_error);
    }
}
