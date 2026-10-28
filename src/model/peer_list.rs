use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use qbittorrent_client::{PeerPatch, PeersData};

mod peer_imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::Peer)]
    pub struct Peer {
        /// `ip:port`, as keyed by the server
        #[property(get, construct_only)]
        key: RefCell<String>,
        #[property(get, set)]
        address: RefCell<String>,
        #[property(get, set)]
        host_name: RefCell<String>,
        #[property(get, set)]
        client: RefCell<String>,
        #[property(get, set)]
        connection: RefCell<String>,
        #[property(get, set)]
        country: RefCell<String>,
        #[property(get, set)]
        progress: Cell<f64>,
        #[property(get, set)]
        download_speed: Cell<i64>,
        #[property(get, set)]
        upload_speed: Cell<i64>,
        #[property(get, set)]
        downloaded: Cell<i64>,
        #[property(get, set)]
        uploaded: Cell<i64>,
        #[property(get, set)]
        flags_description: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Peer {
        const NAME: &'static str = "ParcelsPeer";
        type Type = super::Peer;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Peer {}
}

glib::wrapper! {
    /// A peer connected to a torrent.
    pub struct Peer(ObjectSubclass<peer_imp::Peer>);
}

macro_rules! update {
    ($obj:ident, $value:expr, $getter:ident, $setter:ident) => {
        if let Some(value) = $value {
            if $obj.$getter() != *value {
                $obj.$setter(value.clone());
            }
        }
    };
}

impl Peer {
    fn new(key: &str) -> Self {
        let peer: Self = glib::Object::builder().property("key", key).build();
        // Shown until the server sends the separate fields
        peer.set_address(key);
        peer
    }

    fn apply(&self, patch: &PeerPatch) {
        let _guard = self.freeze_notify();
        if let Some(dest) = patch.i2p_dest.as_ref().filter(|d| !d.is_empty()) {
            self.set_address(dest.clone());
        } else if let (Some(ip), Some(port)) = (&patch.ip, patch.port) {
            // IPv6 addresses need brackets before the port
            let address = if ip.contains(':') {
                format!("[{ip}]:{port}")
            } else {
                format!("{ip}:{port}")
            };
            if self.address() != address {
                self.set_address(address);
            }
        }
        update!(self, &patch.host_name, host_name, set_host_name);
        update!(self, &patch.client, client, set_client);
        update!(self, &patch.connection, connection, set_connection);
        update!(self, &patch.country, country, set_country);
        update!(self, &patch.progress, progress, set_progress);
        update!(self, &patch.dl_speed, download_speed, set_download_speed);
        update!(self, &patch.up_speed, upload_speed, set_upload_speed);
        update!(self, &patch.downloaded, downloaded, set_downloaded);
        update!(self, &patch.uploaded, uploaded, set_uploaded);
        update!(
            self,
            &patch.flags_desc,
            flags_description,
            set_flags_description
        );
    }
}

mod list_imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PeerList {
        pub store: OnceCell<gio::ListStore>,
        pub by_key: RefCell<HashMap<String, super::Peer>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerList {
        const NAME: &'static str = "ParcelsPeerList";
        type Type = super::PeerList;
    }

    impl ObjectImpl for PeerList {
        fn constructed(&self) {
            self.parent_constructed();
            self.store.get_or_init(gio::ListStore::new::<super::Peer>);
        }
    }
}

glib::wrapper! {
    /// The peers of one torrent, fed by `sync/torrentPeers` diffs.
    pub struct PeerList(ObjectSubclass<list_imp::PeerList>);
}

impl Default for PeerList {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PeerList {
    pub fn model(&self) -> &gio::ListStore {
        self.imp().store.get().expect("set in constructed()")
    }

    pub fn apply(&self, data: &PeersData) {
        let store = self.model();
        let mut by_key = self.imp().by_key.borrow_mut();

        let removed: HashSet<String> = if data.full_update {
            by_key
                .keys()
                .filter(|key| !data.peers.contains_key(*key))
                .cloned()
                .collect()
        } else {
            data.peers_removed.iter().cloned().collect()
        };
        if !removed.is_empty() {
            by_key.retain(|key, _| !removed.contains(key));
            let remaining: Vec<Peer> = store
                .iter::<Peer>()
                .filter_map(Result::ok)
                .filter(|peer| !removed.contains(&peer.key()))
                .collect();
            store.splice(0, store.n_items(), &remaining);
        }

        let mut added = Vec::new();
        for (key, patch) in &data.peers {
            match by_key.get(key) {
                Some(peer) => peer.apply(patch),
                None => {
                    let peer = Peer::new(key);
                    peer.apply(patch);
                    by_key.insert(key.clone(), peer.clone());
                    added.push(peer);
                }
            }
        }
        added.sort_by_key(Peer::key);
        store.splice(store.n_items(), 0, &added);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PeersData {
        let path = format!(
            "{}/crates/qbittorrent-client/tests/fixtures/details/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn applies_full_and_partial_updates() {
        let list = PeerList::default();
        list.apply(&fixture("peers-01-full.json"));
        assert_eq!(list.model().n_items(), 1);
        let peer = list.model().item(0).and_downcast::<Peer>().unwrap();
        assert_eq!(peer.address(), "172.19.0.3:6882");
        assert_eq!(peer.client(), "qBittorrent/5.3.0");
        let downloaded = peer.downloaded();

        list.apply(&fixture("peers-02-diff.json"));
        assert_eq!(list.model().n_items(), 1);
        assert!(peer.downloaded() > downloaded, "patched in place");
        assert_eq!(peer.client(), "qBittorrent/5.3.0", "unchanged");

        list.apply(&PeersData {
            peers_removed: vec!["172.19.0.3:6882".into()],
            ..Default::default()
        });
        assert_eq!(list.model().n_items(), 0);
    }

    #[test]
    fn formats_ipv6_and_i2p_addresses() {
        let list = PeerList::default();
        let mut data = PeersData {
            full_update: true,
            ..Default::default()
        };
        data.peers.insert(
            "[::1]:51413".into(),
            PeerPatch {
                ip: Some("::1".into()),
                port: Some(51413),
                ..Default::default()
            },
        );
        data.peers.insert(
            "i2p".into(),
            PeerPatch {
                i2p_dest: Some("abc.b32.i2p".into()),
                ..Default::default()
            },
        );
        list.apply(&data);
        let addresses: HashSet<String> = list
            .model()
            .iter::<Peer>()
            .map(|p| p.unwrap().address())
            .collect();
        assert_eq!(
            addresses,
            HashSet::from(["[::1]:51413".into(), "abc.b32.i2p".into()])
        );
    }
}
