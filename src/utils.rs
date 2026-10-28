//! Localized formatting of sizes, speeds, durations and torrent states.

use gettextrs::gettext;
use gtk::glib;
use qbittorrent_client::TorrentState;

/// Formats a size in bytes, e.g. "1.2 GB".
pub fn format_size(bytes: i64) -> String {
    glib::format_size(bytes.max(0) as u64).to_string()
}

/// Formats a speed in bytes per second, e.g. "1.2 MB/s".
pub fn format_speed(bytes_per_second: i64) -> String {
    // Translators: A transfer speed, e.g. "1.2 MB/s"; {} is the size
    gettext("{}/s").replace("{}", &format_size(bytes_per_second))
}

/// Formats a remaining time compactly, e.g. "2 h 13 min". Shows at most two units.
pub fn format_duration(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (days, hours) = (seconds / 86_400, seconds % 86_400 / 3600);
    let (minutes, secs) = (seconds % 3600 / 60, seconds % 60);

    let fill = |template: String, a: i64, b: i64| {
        template
            .replace("{0}", &a.to_string())
            .replace("{1}", &b.to_string())
    };
    if days > 0 {
        // Translators: Compact duration; {0} is days, {1} is hours
        fill(gettext("{0} d {1} h"), days, hours)
    } else if hours > 0 {
        // Translators: Compact duration; {0} is hours, {1} is minutes
        fill(gettext("{0} h {1} min"), hours, minutes)
    } else if minutes > 0 {
        // Translators: Compact duration; {0} is minutes
        fill(gettext("{0} min"), minutes, 0)
    } else {
        // Translators: Compact duration; {0} is seconds
        fill(gettext("{0} s"), secs, 0)
    }
}

pub fn state_label(state: TorrentState) -> String {
    match state {
        TorrentState::Downloading | TorrentState::ForcedDownloading => gettext("Downloading"),
        TorrentState::DownloadingMetadata | TorrentState::ForcedDownloadingMetadata => {
            gettext("Fetching metadata")
        }
        TorrentState::StalledDownloading => gettext("Stalled"),
        TorrentState::Uploading | TorrentState::ForcedUploading => gettext("Seeding"),
        TorrentState::StalledUploading => gettext("Seeding"),
        TorrentState::StoppedDownloading | TorrentState::StoppedUploading => gettext("Stopped"),
        TorrentState::QueuedDownloading | TorrentState::QueuedUploading => gettext("Queued"),
        TorrentState::CheckingDownloading
        | TorrentState::CheckingUploading
        | TorrentState::CheckingResumeData => gettext("Checking"),
        TorrentState::Allocating => gettext("Allocating"),
        TorrentState::Moving => gettext("Moving"),
        TorrentState::Error => gettext("Error"),
        TorrentState::MissingFiles => gettext("Missing files"),
        TorrentState::Unknown => gettext("Unknown"),
    }
}

pub fn state_icon(state: TorrentState) -> &'static str {
    match state {
        TorrentState::Error | TorrentState::MissingFiles => "dialog-error-symbolic",
        TorrentState::StoppedDownloading | TorrentState::StoppedUploading => {
            "media-playback-pause-symbolic"
        }
        s if s.is_checking() || s.is_queued() => "content-loading-symbolic",
        TorrentState::DownloadingMetadata
        | TorrentState::ForcedDownloadingMetadata
        | TorrentState::Allocating
        | TorrentState::Moving => "content-loading-symbolic",
        s if s.is_seeding() => "network-transmit-symbolic",
        s if s.is_downloading() => "folder-download-symbolic",
        _ => "dialog-question-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(42), "42 s");
        assert_eq!(format_duration(125), "2 min");
        assert_eq!(format_duration(3 * 3600 + 13 * 60 + 5), "3 h 13 min");
        assert_eq!(format_duration(2 * 86_400 + 5 * 3600), "2 d 5 h");
        assert_eq!(format_duration(-5), "0 s");
    }

    #[test]
    fn speeds() {
        // GLib separates units from decimal numbers with a no-break space
        assert_eq!(format_speed(1_500_000), "1.5\u{a0}MB/s");
        assert_eq!(format_speed(-1), "0 bytes/s");
    }
}
