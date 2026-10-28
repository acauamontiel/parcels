//! Parsing of real `sync/maindata` responses, captured from qBittorrent 5.3 and 5.0.

use qbittorrent_client::{MainData, TorrentState};

const NOTES: &str = "a5a1209be56e3bdfa75297d9809240b95358275b";
const ALBUM: &str = "349e3db6ee4dd3094191e013d854bbf9f6d8801a";
const MAGNET: &str = "0123456789abcdef0123456789abcdef01234567";

fn fixture(name: &str) -> MainData {
    let path = format!(
        "{}/tests/fixtures/maindata/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn full_update() {
    let data = fixture("01-full.json");
    assert!(data.full_update);
    assert_eq!(data.rid, 1);
    assert_eq!(data.torrents.len(), 3);
    assert_eq!(data.tags, ["iso", "weekend"]);
    assert_eq!(
        data.categories["music"].save_path.as_deref(),
        Some("/downloads/music")
    );

    let magnet = &data.torrents[MAGNET];
    assert_eq!(magnet.name.as_deref(), Some("Missing Linux ISO"));
    assert_eq!(magnet.state, Some(TorrentState::DownloadingMetadata));
    assert_eq!(magnet.tags.as_deref(), Some("iso, weekend"));
    assert_eq!(magnet.total_size, Some(-1));

    let notes = &data.torrents[NOTES];
    assert_eq!(notes.state, Some(TorrentState::StalledUploading));
    assert_eq!(notes.progress, Some(1.0));

    let state = data.server_state.unwrap();
    assert_eq!(state.use_alt_speed_limits, Some(false));
    assert!(state.free_space_on_disk.unwrap() > 0);
}

#[test]
fn partial_update_only_has_changed_fields() {
    let data = fixture("02-category-and-tag.json");
    assert!(!data.full_update);
    assert_eq!(data.torrents[ALBUM].category.as_deref(), Some("music"));
    assert_eq!(data.torrents[ALBUM].name, None);
    assert_eq!(data.torrents[NOTES].tags.as_deref(), Some("docs"));
    assert_eq!(data.tags, ["docs"]);
    assert!(data.server_state.is_none());
}

#[test]
fn stopped_torrent() {
    let data = fixture("03-stopped.json");
    assert_eq!(
        data.torrents[ALBUM].state,
        Some(TorrentState::StoppedUploading)
    );
    assert_eq!(data.torrents[NOTES].state, None);
}

#[test]
fn removals_and_new_category() {
    let data = fixture("04-removed-and-new-category.json");
    assert_eq!(data.torrents_removed, [MAGNET]);
    assert_eq!(data.tags_removed, ["weekend"]);
    assert!(data.categories.contains_key("video"));
    assert!(data.torrents.is_empty());
}

#[test]
fn no_changes() {
    let data = fixture("05-no-changes.json");
    assert_eq!(
        data,
        MainData {
            rid: 5,
            ..Default::default()
        }
    );
}

#[test]
fn qbittorrent_5_0() {
    let data = fixture("50-full-qbittorrent-5.0.json");
    assert!(data.full_update);
    let torrent = data.torrents.values().next().unwrap();
    assert_eq!(torrent.state, Some(TorrentState::StoppedDownloading));
    assert_eq!(torrent.category.as_deref(), Some("linux"));
}
