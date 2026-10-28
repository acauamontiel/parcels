use std::cell::{Cell, OnceCell, RefCell};

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use qbittorrent_client::{FilePriority, TorrentFile};

/// [`FilePriority`], registered with GObject so it can be a property.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ParcelsFilePriority")]
pub enum Priority {
    Skip,
    #[default]
    Normal,
    High,
    Maximum,
}

impl From<FilePriority> for Priority {
    fn from(priority: FilePriority) -> Self {
        match priority {
            FilePriority::Skip => Self::Skip,
            FilePriority::Normal => Self::Normal,
            FilePriority::High => Self::High,
            FilePriority::Maximum => Self::Maximum,
        }
    }
}

impl From<Priority> for FilePriority {
    fn from(priority: Priority) -> Self {
        match priority {
            Priority::Skip => Self::Skip,
            Priority::Normal => Self::Normal,
            Priority::High => Self::High,
            Priority::Maximum => Self::Maximum,
        }
    }
}

mod item_imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::FileItem)]
    pub struct FileItem {
        #[property(get, construct_only)]
        index: Cell<u32>,
        /// Path inside the torrent
        #[property(get, set)]
        path: RefCell<String>,
        #[property(get, set)]
        size: Cell<i64>,
        #[property(get, set)]
        progress: Cell<f64>,
        #[property(get, set, builder(Priority::default()))]
        priority: Cell<Priority>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileItem {
        const NAME: &'static str = "ParcelsFileItem";
        type Type = super::FileItem;
    }

    #[glib::derived_properties]
    impl ObjectImpl for FileItem {}
}

glib::wrapper! {
    /// A file of a torrent.
    pub struct FileItem(ObjectSubclass<item_imp::FileItem>);
}

impl FileItem {
    /// The last path component.
    pub fn name(&self) -> String {
        let path = self.path();
        path.rsplit('/').next().unwrap_or(&path).to_owned()
    }

    /// The folders above the file, without the top folder shared by all files.
    pub fn folder(&self) -> String {
        let path = self.path();
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop();
        if parts.len() > 1 {
            parts.remove(0);
            parts.join("/")
        } else {
            String::new()
        }
    }

    fn apply(&self, file: &TorrentFile) {
        let _guard = self.freeze_notify();
        if self.path() != file.name {
            self.set_path(file.name.clone());
        }
        if self.size() != file.size {
            self.set_size(file.size);
        }
        if self.progress() != file.progress {
            self.set_progress(file.progress);
        }
        let priority = Priority::from(file.priority());
        if self.priority() != priority {
            self.set_priority(priority);
        }
    }
}

mod list_imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct FileList {
        pub store: OnceCell<gio::ListStore>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileList {
        const NAME: &'static str = "ParcelsFileList";
        type Type = super::FileList;
    }

    impl ObjectImpl for FileList {
        fn constructed(&self) {
            self.parent_constructed();
            self.store
                .get_or_init(gio::ListStore::new::<super::FileItem>);
        }
    }
}

glib::wrapper! {
    /// The files of a torrent, sorted by path and updated in place.
    pub struct FileList(ObjectSubclass<list_imp::FileList>);
}

impl Default for FileList {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl FileList {
    pub fn model(&self) -> &gio::ListStore {
        self.imp().store.get().expect("set in constructed()")
    }

    pub fn apply(&self, files: &[TorrentFile]) {
        let store = self.model();
        let same_files = store.n_items() as usize == files.len()
            && store
                .iter::<FileItem>()
                .filter_map(Result::ok)
                .all(|item| files.iter().any(|f| f.index == item.index()));

        if same_files {
            for item in store.iter::<FileItem>().filter_map(Result::ok) {
                if let Some(file) = files.iter().find(|f| f.index == item.index()) {
                    item.apply(file);
                }
            }
            return;
        }

        let mut items: Vec<FileItem> = files
            .iter()
            .map(|file| {
                let item: FileItem = glib::Object::builder()
                    .property("index", file.index)
                    .build();
                item.apply(file);
                item
            })
            .collect();
        items.sort_by_key(|item| item.path().to_lowercase());
        store.splice(0, store.n_items(), &items);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<TorrentFile> {
        let path = format!(
            "{}/crates/qbittorrent-client/tests/fixtures/details/files.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn sorts_and_updates_in_place() {
        let list = FileList::default();
        let mut files = fixture();
        list.apply(&files);
        assert_eq!(list.model().n_items(), 5);

        let first = list.model().item(0).and_downcast::<FileItem>().unwrap();
        assert_eq!(
            first.path(),
            "Blender Open Movies/Big Buck Bunny/bbb-1080p.mp4"
        );
        assert_eq!(first.name(), "bbb-1080p.mp4");
        assert_eq!(first.folder(), "Big Buck Bunny");

        let index = first.index();
        let file = files.iter_mut().find(|f| f.index == index).unwrap();
        file.progress = 0.5;
        list.apply(&files);
        let same = list.model().item(0).and_downcast::<FileItem>().unwrap();
        assert!(same == first, "same object");
        assert_eq!(first.progress(), 0.5);
    }

    #[test]
    fn top_level_files_have_no_folder() {
        let item: FileItem = glib::Object::builder().property("index", 0u32).build();
        item.set_path("single.iso");
        assert_eq!(item.folder(), "");
        item.set_path("Album/track.flac");
        assert_eq!(item.folder(), "");
        item.set_path("Album/CD 1/track.flac");
        assert_eq!(item.folder(), "CD 1");
    }
}
