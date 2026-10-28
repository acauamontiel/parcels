//! Parsing of real detail responses, captured from qBittorrent 5.3 while a
//! multi-file torrent was downloading from a peer.

use qbittorrent_client::{
    FilePriority, PeersData, TorrentFile, TorrentProperties, Tracker, TrackerStatus,
};

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    let path = format!(
        "{}/tests/fixtures/details/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn properties() {
    let properties: TorrentProperties = fixture("properties.json");
    assert_eq!(properties.save_path, "/downloads/peer");
    assert_eq!(properties.total_size, 65_536_000);
    assert_eq!(properties.pieces_num, 500);
    assert_eq!(properties.completion_date, -1);
    assert_eq!(properties.created_by, "qBittorrent v5.3.0");
    assert!(!properties.is_private);
    assert_eq!(properties.has_metadata, Some(true));
    assert_eq!(properties.infohash_v1.len(), 40);
    assert_eq!(properties.infohash_v2.len(), 64);

    let magnet: TorrentProperties = fixture("properties-no-metadata.json");
    assert_eq!(magnet.has_metadata, Some(false));
}

#[test]
fn files() {
    let files: Vec<TorrentFile> = fixture("files.json");
    assert_eq!(files.len(), 5);
    assert_eq!(
        files[0].name,
        "Blender Open Movies/Big Buck Bunny/bbb-1080p.mp4"
    );
    assert_eq!(files[2].index, 2);
    assert_eq!(files[2].priority(), FilePriority::Normal);
    assert!(files.iter().all(|f| (0.0..=1.0).contains(&f.progress)));

    let none: Vec<TorrentFile> = fixture("files-no-metadata.json");
    assert!(none.is_empty());
}

#[test]
fn trackers() {
    let trackers: Vec<Tracker> = fixture("trackers.json");
    let sources: Vec<_> = trackers.iter().filter_map(Tracker::pseudo_source).collect();
    assert_eq!(sources, ["DHT", "PeX", "LSD"]);

    let tracker = trackers.last().unwrap();
    assert_eq!(tracker.url, "http://tracker.invalid:6969/announce");
    assert_eq!(tracker.tier, 0);
    assert_eq!(tracker.status(), TrackerStatus::NotWorking);
    assert_eq!(tracker.num_peers, -1);
    assert_eq!(tracker.msg, "Host not found (authoritative)");
}

#[test]
fn peers() {
    let full: PeersData = fixture("peers-01-full.json");
    assert!(full.full_update);
    let peer = &full.peers["172.19.0.3:6882"];
    assert_eq!(peer.client.as_deref(), Some("qBittorrent/5.3.0"));
    assert_eq!(peer.port, Some(6882));
    assert_eq!(peer.country_code, None, "null country");
    assert_eq!(peer.progress, Some(1.0));

    let diff: PeersData = fixture("peers-02-diff.json");
    assert!(!diff.full_update);
    let peer = &diff.peers["172.19.0.3:6882"];
    assert!(peer.dl_speed.is_some());
    assert_eq!(peer.client, None, "unchanged fields are absent");
}
