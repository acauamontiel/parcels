use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::glib::subclass::Signal;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use qbittorrent_client::MainData;
use qbittorrent_client::maindata::ServerStatePatch;

use super::Torrent;

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::TorrentList)]
    pub struct TorrentList {
        pub store: OnceCell<gio::ListStore>,
        pub by_hash: RefCell<HashMap<String, Torrent>>,
        /// Category name → save path
        pub categories: RefCell<BTreeMap<String, String>>,
        pub tags: RefCell<BTreeSet<String>>,

        /// Global download speed, bytes per second
        #[property(get)]
        pub download_speed: Cell<i64>,
        #[property(get)]
        pub upload_speed: Cell<i64>,
        /// Alternative speed limits are on
        #[property(get)]
        pub alt_speed_limits: Cell<bool>,
        /// Free space in the default save path, bytes
        #[property(get)]
        pub free_space: Cell<i64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentList {
        const NAME: &'static str = "ParcelsTorrentList";
        type Type = super::TorrentList;
    }

    #[glib::derived_properties]
    impl ObjectImpl for TorrentList {
        fn constructed(&self) {
            self.parent_constructed();
            self.store.get_or_init(gio::ListStore::new::<Torrent>);
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // After every applied update: filters, sorting and counts may change
                    Signal::builder("updated").build(),
                    // The set of categories or tags changed
                    Signal::builder("labels-changed").build(),
                    // A torrent that was downloading finished
                    Signal::builder("torrent-completed")
                        .param_types([Torrent::static_type()])
                        .build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    /// All torrents of the connected server, indexed by hash.
    ///
    /// Fed by `sync/maindata` diffs through [`TorrentList::apply`]. Torrent
    /// objects are updated in place, so rows and selections survive updates.
    pub struct TorrentList(ObjectSubclass<imp::TorrentList>);
}

impl Default for TorrentList {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl TorrentList {
    /// The torrents, as a list model of [`Torrent`].
    pub fn model(&self) -> &gio::ListStore {
        self.imp().store.get().expect("set in constructed()")
    }

    pub fn get(&self, hash: &str) -> Option<Torrent> {
        self.imp().by_hash.borrow().get(hash).cloned()
    }

    pub fn len(&self) -> u32 {
        self.model().n_items()
    }

    /// Category names, sorted.
    pub fn categories(&self) -> Vec<String> {
        self.imp().categories.borrow().keys().cloned().collect()
    }

    #[allow(dead_code)] // Used by the add dialog and "move" action in later phases
    pub fn category_save_path(&self, category: &str) -> Option<String> {
        self.imp().categories.borrow().get(category).cloned()
    }

    /// Tags, sorted.
    pub fn tags(&self) -> Vec<String> {
        self.imp().tags.borrow().iter().cloned().collect()
    }

    pub fn connect_updated<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("updated", false, move |args| {
            f(&args[0].get().expect("signal emitter"));
            None
        })
    }

    pub fn connect_labels_changed<F: Fn(&Self) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_local("labels-changed", false, move |args| {
            f(&args[0].get().expect("signal emitter"));
            None
        })
    }

    pub fn connect_torrent_completed<F: Fn(&Self, &Torrent) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("torrent-completed", false, move |args| {
            let list: Self = args[0].get().expect("signal emitter");
            f(&list, &args[1].get::<Torrent>().expect("torrent"));
            None
        })
    }

    /// Forgets everything, e.g. when switching servers.
    pub fn clear(&self) {
        let imp = self.imp();
        imp.by_hash.borrow_mut().clear();
        self.model().remove_all();
        let had_labels = !imp.categories.borrow().is_empty() || !imp.tags.borrow().is_empty();
        imp.categories.borrow_mut().clear();
        imp.tags.borrow_mut().clear();
        self.apply_server_state(&ServerStatePatch {
            dl_info_speed: Some(0),
            up_info_speed: Some(0),
            use_alt_speed_limits: Some(false),
            free_space_on_disk: Some(0),
            ..Default::default()
        });
        if had_labels {
            self.emit_by_name::<()>("labels-changed", &[]);
        }
        self.emit_by_name::<()>("updated", &[]);
    }

    /// Applies a `sync/maindata` response.
    pub fn apply(&self, data: &MainData) {
        let imp = self.imp();

        if data.full_update {
            // Keep the objects of torrents that still exist, drop the others
            let keep: HashSet<&str> = data.torrents.keys().map(String::as_str).collect();
            let removed: Vec<String> = imp
                .by_hash
                .borrow()
                .keys()
                .filter(|hash| !keep.contains(hash.as_str()))
                .cloned()
                .collect();
            self.remove(&removed);
        } else {
            self.remove(&data.torrents_removed);
        }

        let mut added = Vec::new();
        let mut completed = Vec::new();
        {
            let mut by_hash = imp.by_hash.borrow_mut();
            for (hash, patch) in &data.torrents {
                match by_hash.get(hash) {
                    Some(torrent) => {
                        // Only torrents seen downloading count: new ones that are
                        // already complete (e.g. on connect) don't
                        let was_complete = is_complete(torrent);
                        torrent.apply(patch);
                        if !was_complete && is_complete(torrent) {
                            completed.push(torrent.clone());
                        }
                    }
                    None => {
                        let torrent = Torrent::new(hash);
                        torrent.apply(patch);
                        by_hash.insert(hash.clone(), torrent.clone());
                        added.push(torrent);
                    }
                }
            }
        }
        if !added.is_empty() {
            // Stable order for new torrents: oldest first
            added.sort_by_key(|t| (t.added_on(), t.hash()));
            let position = self.model().n_items();
            self.model().splice(position, 0, &added);
        }

        if self.apply_labels(data) {
            self.emit_by_name::<()>("labels-changed", &[]);
        }
        if let Some(state) = &data.server_state {
            self.apply_server_state(state);
        }

        self.emit_by_name::<()>("updated", &[]);
        for torrent in completed {
            self.emit_by_name::<()>("torrent-completed", &[&torrent]);
        }
    }

    fn remove(&self, hashes: &[String]) {
        if hashes.is_empty() {
            return;
        }

        let mut by_hash = self.imp().by_hash.borrow_mut();
        let removed: HashSet<&str> = hashes
            .iter()
            .filter(|hash| by_hash.remove(hash.as_str()).is_some())
            .map(String::as_str)
            .collect();
        if removed.is_empty() {
            return;
        }

        let store = self.model();
        let remaining: Vec<Torrent> = store
            .iter::<Torrent>()
            .filter_map(Result::ok)
            .filter(|torrent| !removed.contains(torrent.hash().as_str()))
            .collect();
        store.splice(0, store.n_items(), &remaining);
    }

    /// Returns whether categories or tags changed.
    fn apply_labels(&self, data: &MainData) -> bool {
        let imp = self.imp();
        let mut categories = imp.categories.borrow_mut();
        let mut tags = imp.tags.borrow_mut();
        let before = (categories.clone(), tags.clone());

        if data.full_update {
            categories.clear();
            tags.clear();
        }
        for (name, patch) in &data.categories {
            let entry = categories.entry(name.clone()).or_default();
            if let Some(path) = &patch.save_path {
                *entry = path.clone();
            }
        }
        for name in &data.categories_removed {
            categories.remove(name);
        }
        tags.extend(data.tags.iter().cloned());
        for tag in &data.tags_removed {
            tags.remove(tag);
        }

        before != (categories.clone(), tags.clone())
    }

    fn apply_server_state(&self, state: &ServerStatePatch) {
        let imp = self.imp();
        let _guard = self.freeze_notify();
        if let Some(v) = state.dl_info_speed
            && imp.download_speed.replace(v) != v
        {
            self.notify_download_speed();
        }
        if let Some(v) = state.up_info_speed
            && imp.upload_speed.replace(v) != v
        {
            self.notify_upload_speed();
        }
        if let Some(v) = state.use_alt_speed_limits
            && imp.alt_speed_limits.replace(v) != v
        {
            self.notify_alt_speed_limits();
        }
        if let Some(v) = state.free_space_on_disk
            && imp.free_space.replace(v) != v
        {
            self.notify_free_space();
        }
    }
}

/// All selected files are downloaded. Torrents without metadata aren't complete.
fn is_complete(torrent: &Torrent) -> bool {
    torrent.has_metadata() && torrent.progress() >= 1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::torrent::TorrentStatus;

    const NOTES: &str = "a5a1209be56e3bdfa75297d9809240b95358275b";
    const ALBUM: &str = "349e3db6ee4dd3094191e013d854bbf9f6d8801a";
    const MAGNET: &str = "0123456789abcdef0123456789abcdef01234567";

    fn fixture(name: &str) -> MainData {
        let path = format!(
            "{}/crates/qbittorrent-client/tests/fixtures/maindata/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn hashes(list: &TorrentList) -> Vec<String> {
        list.model()
            .iter::<Torrent>()
            .map(|t| t.unwrap().hash())
            .collect()
    }

    #[test]
    fn full_update_fills_the_list() {
        let list = TorrentList::default();
        list.apply(&fixture("01-full.json"));

        assert_eq!(list.len(), 3);
        let magnet = list.get(MAGNET).unwrap();
        assert_eq!(magnet.name(), "Missing Linux ISO");
        assert_eq!(magnet.status(), TorrentStatus::DownloadingMetadata);
        assert_eq!(magnet.eta(), -1, "ETA_INFINITY becomes -1");
        assert_eq!(magnet.tag_list(), ["iso", "weekend"]);
        assert_eq!(list.categories(), ["linux", "music"]);
        assert_eq!(
            list.category_save_path("music").as_deref(),
            Some("/downloads/music")
        );
        assert_eq!(list.tags(), ["iso", "weekend"]);
        assert!(list.free_space() > 0);
    }

    #[test]
    fn partial_updates_patch_objects_in_place() {
        let list = TorrentList::default();
        list.apply(&fixture("01-full.json"));
        let album = list.get(ALBUM).unwrap();
        let name_before = album.name();

        let notified = std::rc::Rc::new(Cell::new(0));
        album.connect_notify_local(
            None,
            glib::clone!(
                #[strong]
                notified,
                move |_, _| notified.set(notified.get() + 1)
            ),
        );

        list.apply(&fixture("02-category-and-tag.json"));
        assert_eq!(album.category(), "music");
        assert_eq!(album.name(), name_before, "absent fields are unchanged");
        assert_eq!(notified.get(), 1, "only the changed property notifies");
        assert!(list.get(NOTES).unwrap().has_tag("docs"));
        assert_eq!(list.tags(), ["docs", "iso", "weekend"]);

        list.apply(&fixture("03-stopped.json"));
        assert_eq!(album.status(), TorrentStatus::StoppedUploading);
        assert!(
            list.get(ALBUM).unwrap() == album,
            "same object after updates"
        );
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn removals_and_new_categories() {
        let list = TorrentList::default();
        let labels_changed = std::rc::Rc::new(Cell::new(0));
        list.connect_labels_changed(glib::clone!(
            #[strong]
            labels_changed,
            move |_| labels_changed.set(labels_changed.get() + 1)
        ));

        for name in [
            "01-full.json",
            "02-category-and-tag.json",
            "03-stopped.json",
            "04-removed-and-new-category.json",
        ] {
            list.apply(&fixture(name));
        }

        assert!(list.get(MAGNET).is_none());
        assert_eq!(list.len(), 2);
        assert!(!hashes(&list).contains(&MAGNET.to_owned()));
        assert_eq!(list.categories(), ["linux", "music", "video"]);
        assert_eq!(list.tags(), ["docs", "iso"]);
        // 01 adds labels, 02 adds a tag, 03 none, 04 adds a category and removes a tag
        assert_eq!(labels_changed.get(), 3);

        let before = hashes(&list);
        list.apply(&fixture("05-no-changes.json"));
        assert_eq!(hashes(&list), before);
        assert_eq!(labels_changed.get(), 3);
    }

    #[test]
    fn full_update_replaces_everything() {
        let list = TorrentList::default();
        list.apply(&fixture("01-full.json"));
        let notes = list.get(NOTES).unwrap();

        // A full snapshot without the magnet torrent, e.g. after the server restarted
        let mut snapshot = fixture("01-full.json");
        snapshot.torrents.remove(MAGNET);
        snapshot.tags = vec!["iso".into()];
        list.apply(&snapshot);

        assert_eq!(list.len(), 2);
        assert!(
            list.get(NOTES).unwrap() == notes,
            "existing objects are reused"
        );
        assert_eq!(list.tags(), ["iso"]);
    }

    #[test]
    fn works_with_qbittorrent_5_0() {
        let list = TorrentList::default();
        list.apply(&fixture("50-full-qbittorrent-5.0.json"));
        let torrent = list.model().item(0).and_downcast::<Torrent>().unwrap();
        assert_eq!(torrent.status(), TorrentStatus::StoppedDownloading);
        assert_eq!(torrent.category(), "linux");
    }

    #[test]
    fn reports_finished_downloads_once() {
        use qbittorrent_client::maindata::TorrentPatch;

        let list = TorrentList::default();
        let completed = std::rc::Rc::new(RefCell::new(Vec::new()));
        list.connect_torrent_completed(glib::clone!(
            #[strong]
            completed,
            move |_, torrent| completed.borrow_mut().push(torrent.hash())
        ));

        // Already complete when first seen: not reported
        list.apply(&fixture("01-full.json"));
        assert!(completed.borrow().is_empty());

        let patch = |progress: f64| {
            let mut data = MainData {
                rid: 2,
                ..Default::default()
            };
            data.torrents.insert(
                "new".into(),
                TorrentPatch {
                    name: Some("New".into()),
                    progress: Some(progress),
                    has_metadata: Some(true),
                    ..Default::default()
                },
            );
            data
        };
        list.apply(&patch(0.5));
        list.apply(&patch(1.0));
        list.apply(&patch(1.0));
        assert_eq!(*completed.borrow(), ["new"]);
    }

    #[test]
    fn clear_forgets_everything() {
        let list = TorrentList::default();
        list.apply(&fixture("01-full.json"));
        list.clear();
        assert_eq!(list.len(), 0);
        assert!(list.categories().is_empty());
        assert_eq!(list.free_space(), 0);
    }
}
