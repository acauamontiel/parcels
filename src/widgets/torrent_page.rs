use std::cell::{Cell, OnceCell, RefCell};
use std::collections::HashMap;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib;
use gtk::glib::clone;
use qbittorrent_client::{Error, TorrentProperties, Tracker, TrackerStatus};

use crate::model::{FileItem, FileList, Peer, PeerList, Priority, Torrent};
use crate::session::Session;
use crate::utils;
use crate::widgets::detail_rows::{FileRow, PeerRow};

/// Rows of the General tab, in display order: (key, group).
const GENERAL_ROWS: &[(&str, &str)] = &[
    ("status", "transfer"),
    ("progress", "transfer"),
    ("eta", "transfer"),
    ("download", "transfer"),
    ("upload", "transfer"),
    ("downloaded", "transfer"),
    ("uploaded", "transfer"),
    ("ratio", "transfer"),
    ("seeds", "transfer"),
    ("peers", "transfer"),
    ("wasted", "transfer"),
    ("size", "torrent"),
    ("pieces", "torrent"),
    ("save-path", "torrent"),
    ("added", "torrent"),
    ("completed", "torrent"),
    ("created", "torrent"),
    ("hash-v1", "torrent"),
    ("hash-v2", "torrent"),
    ("private", "torrent"),
    ("comment", "torrent"),
];

/// The titles above, translated at runtime.
fn row_title(key: &str) -> String {
    match key {
        "status" => gettext("Status"),
        "progress" => gettext("Progress"),
        "eta" => gettext("Time Remaining"),
        "download" => gettext("Download Speed"),
        "upload" => gettext("Upload Speed"),
        "downloaded" => gettext("Downloaded"),
        "uploaded" => gettext("Uploaded"),
        "ratio" => gettext("Ratio"),
        "seeds" => gettext("Seeds"),
        "peers" => gettext("Peers"),
        "wasted" => gettext("Wasted"),
        "size" => gettext("Size"),
        "pieces" => gettext("Pieces"),
        "save-path" => gettext("Save Folder"),
        "added" => gettext("Added On"),
        "completed" => gettext("Completed On"),
        "created" => gettext("Created"),
        "hash-v1" => gettext("Info Hash v1"),
        "hash-v2" => gettext("Info Hash v2"),
        "private" => gettext("Private"),
        "comment" => gettext("Comment"),
        _ => key.to_owned(),
    }
}

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/torrent-page.ui")]
    pub struct TorrentPage {
        #[template_child]
        pub view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub transfer_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub torrent_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub files_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub files_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub trackers_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub peers_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub peers_view: TemplateChild<gtk::ListView>,

        pub torrent: OnceCell<Torrent>,
        pub session: OnceCell<Session>,
        pub rows: RefCell<HashMap<&'static str, adw::ActionRow>>,
        pub properties: RefCell<Option<TorrentProperties>>,
        pub files: OnceCell<FileList>,
        pub peers: OnceCell<PeerList>,
        pub peers_rid: Cell<i64>,
        pub trackers: RefCell<Vec<Tracker>>,
        pub tracker_rows: RefCell<Vec<adw::ActionRow>>,
        pub refresh_requested: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentPage {
        const NAME: &'static str = "ParcelsTorrentPage";
        type Type = super::TorrentPage;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentPage {}
    impl WidgetImpl for TorrentPage {}
    impl NavigationPageImpl for TorrentPage {}
}

glib::wrapper! {
    /// Details of one torrent: general information, files, trackers and peers.
    pub struct TorrentPage(ObjectSubclass<imp::TorrentPage>)
        @extends gtk::Widget, adw::NavigationPage,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl TorrentPage {
    pub fn new(torrent: &Torrent, session: &Session) -> Self {
        let page: Self = glib::Object::new();
        let imp = page.imp();
        imp.torrent.set(torrent.clone()).expect("set once");
        imp.session.set(session.clone()).expect("set once");
        imp.files.set(FileList::default()).expect("set once");
        imp.peers.set(PeerList::default()).expect("set once");

        torrent
            .bind_property("name", &page, "title")
            .sync_create()
            .build();
        torrent
            .bind_property("name", &*imp.name_label, "label")
            .sync_create()
            .build();
        // The list's live data covers the transfer rows between detail refreshes
        torrent.connect_notify_local(
            None,
            clone!(
                #[weak]
                page,
                move |_, _| page.update_general()
            ),
        );

        page.build_general_rows();
        page.setup_files();
        page.setup_peers();
        page.update_general();
        page.start_refreshing();
        page
    }

    pub fn torrent(&self) -> &Torrent {
        self.imp().torrent.get().expect("set in new()")
    }

    fn session(&self) -> &Session {
        self.imp().session.get().expect("set in new()")
    }

    fn build_general_rows(&self) {
        let imp = self.imp();
        let mut rows = HashMap::new();
        for (key, group) in GENERAL_ROWS {
            let row = adw::ActionRow::builder()
                .title(row_title(key))
                .subtitle_selectable(true)
                .use_markup(false)
                .css_classes(["property"])
                .build();
            match *group {
                "transfer" => imp.transfer_group.add(&row),
                _ => imp.torrent_group.add(&row),
            }
            rows.insert(*key, row);
        }
        imp.rows.replace(rows);
    }

    fn set_row(&self, key: &str, value: Option<String>) {
        if let Some(row) = self.imp().rows.borrow().get(key) {
            let value = value.filter(|v| !v.is_empty());
            row.set_visible(value.is_some());
            row.set_subtitle(value.as_deref().unwrap_or_default());
        }
    }

    fn update_general(&self) {
        let torrent = self.torrent();
        let state = torrent.state();
        let properties = self.imp().properties.borrow();
        let properties = properties.as_ref();

        self.set_row(
            "status",
            Some(match torrent.error() {
                error if state.is_errored() && !error.is_empty() => {
                    format!("{} — {error}", utils::state_label(state))
                }
                _ => utils::state_label(state),
            }),
        );
        let size = torrent.size();
        let percent = (torrent.progress() * 100.0).floor() as i64;
        self.set_row(
            "progress",
            Some(
                // Translators: {done} and {total} are sizes, {percent} a number, e.g. "1.2 GB of 4.5 GB (27%)"
                gettext("{done} of {total} ({percent}%)")
                    .replace("{done}", &utils::format_size(size - torrent.amount_left()))
                    .replace("{total}", &utils::format_size(size))
                    .replace("{percent}", &percent.to_string()),
            ),
        );
        self.set_row(
            "eta",
            (torrent.eta() >= 0 && !state.is_completed() && !state.is_stopped())
                .then(|| utils::format_duration(torrent.eta())),
        );
        self.set_row(
            "download",
            Some(speed_with_average(
                torrent.download_speed(),
                properties.map(|p| p.dl_speed_avg),
            )),
        );
        self.set_row(
            "upload",
            Some(speed_with_average(
                torrent.upload_speed(),
                properties.map(|p| p.up_speed_avg),
            )),
        );
        self.set_row("downloaded", Some(utils::format_size(torrent.downloaded())));
        self.set_row("uploaded", Some(utils::format_size(torrent.uploaded())));
        self.set_row("ratio", Some(format_ratio(torrent.ratio())));
        self.set_row(
            "seeds",
            Some(connected_of_total(torrent.seeds(), torrent.seeds_total())),
        );
        self.set_row(
            "peers",
            Some(connected_of_total(torrent.peers(), torrent.peers_total())),
        );
        self.set_row(
            "wasted",
            properties
                .filter(|p| p.total_wasted > 0)
                .map(|p| utils::format_size(p.total_wasted)),
        );

        self.set_row(
            "size",
            (size > 0).then(|| utils::format_size(torrent.total_size())),
        );
        self.set_row(
            "pieces",
            properties.filter(|p| p.pieces_num > 0).map(|p| {
                // Translators: e.g. "500 × 128 kB (have 54)"
                gettext("{count} × {size} (have {have})")
                    .replace("{count}", &p.pieces_num.to_string())
                    .replace("{size}", &utils::format_size(p.piece_size))
                    .replace("{have}", &p.pieces_have.to_string())
            }),
        );
        self.set_row("save-path", Some(torrent.save_path()));
        self.set_row("added", utils::format_date(torrent.added_on()));
        self.set_row("completed", utils::format_date(torrent.completion_on()));
        self.set_row(
            "created",
            properties.and_then(|p| {
                let date = utils::format_date(p.creation_date);
                match (date, p.created_by.as_str()) {
                    (Some(date), "") => Some(date),
                    // Translators: {date} is a date, {program} the program that created the torrent
                    (Some(date), program) => Some(
                        gettext("{date} by {program}")
                            .replace("{date}", &date)
                            .replace("{program}", program),
                    ),
                    (None, "") => None,
                    (None, program) => Some(program.to_owned()),
                }
            }),
        );
        self.set_row("hash-v1", properties.map(|p| p.infohash_v1.clone()));
        self.set_row("hash-v2", properties.map(|p| p.infohash_v2.clone()));
        self.set_row(
            "private",
            properties.map(|p| {
                if p.is_private {
                    gettext("Yes")
                } else {
                    gettext("No")
                }
            }),
        );
        self.set_row("comment", properties.map(|p| p.comment.clone()));
    }

    fn setup_files(&self) {
        let imp = self.imp();
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(clone!(
            #[weak(rename_to = page)]
            self,
            move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
                let row = FileRow::default();
                row.connect_priority_changed(clone!(
                    #[weak]
                    page,
                    move |row, priority| {
                        if let Some(file) = row.file() {
                            page.set_file_priority(&file, priority);
                        }
                    }
                ));
                item.set_child(Some(&row));
                item.set_activatable(false);
            }
        ));
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            let row = item.child().and_downcast::<FileRow>().expect("a file row");
            row.set_file(item.item().and_downcast_ref::<FileItem>());
        });
        factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            if let Some(row) = item.child().and_downcast::<FileRow>() {
                row.set_file(None);
            }
        });

        let model = imp.files.get().expect("set in new()").model().clone();
        imp.files_view
            .set_model(Some(&gtk::NoSelection::new(Some(model))));
        imp.files_view.set_factory(Some(&factory));
    }

    fn setup_peers(&self) {
        let imp = self.imp();
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            item.set_child(Some(&PeerRow::default()));
            item.set_activatable(false);
        });
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            let row = item.child().and_downcast::<PeerRow>().expect("a peer row");
            row.set_peer(item.item().and_downcast_ref::<Peer>());
        });
        factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            if let Some(row) = item.child().and_downcast::<PeerRow>() {
                row.set_peer(None);
            }
        });

        let model = imp.peers.get().expect("set in new()").model().clone();
        imp.peers_view
            .set_model(Some(&gtk::NoSelection::new(Some(model))));
        imp.peers_view.set_factory(Some(&factory));
    }

    fn set_file_priority(&self, file: &FileItem, priority: Priority) {
        let hash = self.torrent().hash();
        let index = file.index();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = page)]
            self,
            async move {
                let result = page
                    .session()
                    .call(move |client| async move {
                        client
                            .set_file_priority(&hash, &[index], priority.into())
                            .await
                    })
                    .await;
                if let Err(message) = result {
                    page.show_toast(&message);
                }
                // Show the server's state, also after a failure
                page.imp().refresh_requested.set(true);
            }
        ));
    }

    fn show_toast(&self, message: &str) {
        if let Some(overlay) = self
            .ancestor(adw::ToastOverlay::static_type())
            .and_downcast::<adw::ToastOverlay>()
        {
            overlay.add_toast(
                adw::Toast::builder()
                    .title(message)
                    .use_markup(false)
                    .build(),
            );
        }
    }

    #[template_callback]
    fn on_page_changed(&self) {
        // Fetch the newly visible tab right away
        self.imp().refresh_requested.set(true);
    }

    /// Refreshes the visible tab while the page is shown. Stops when the page
    /// is popped or the torrent is gone.
    fn start_refreshing(&self) {
        let weak = self.downgrade();
        glib::spawn_future_local(async move {
            loop {
                let Some(page) = weak.upgrade() else { return };
                // Nothing to do while the page is hidden behind another one
                if page.is_mapped() {
                    match page.refresh().await {
                        Ok(()) => (),
                        Err(Error::TorrentNotFound) => {
                            page.close_removed();
                            return;
                        }
                        // Connection problems are reported by the session's banner
                        Err(err) => tracing::debug!("Details refresh failed: {err}"),
                    }
                }
                let interval = page.session().refresh_interval();
                drop(page);

                // Wait, waking up early on request (tab change, priority change)
                let mut waited = Duration::ZERO;
                while waited < interval {
                    glib::timeout_future(Duration::from_millis(250)).await;
                    waited += Duration::from_millis(250);
                    let Some(page) = weak.upgrade() else { return };
                    if page.imp().refresh_requested.replace(false) {
                        break;
                    }
                }
            }
        });
    }

    async fn refresh(&self) -> Result<(), Error> {
        let Some(client) = self.session().client() else {
            return Ok(());
        };
        let hash = self.torrent().hash();
        let imp = self.imp();

        match imp.view_stack.visible_child_name().as_deref() {
            Some("files") => {
                let files = crate::runtime::run(async move { client.files(&hash).await }).await?;
                imp.files.get().expect("set in new()").apply(&files);
                imp.files_stack.set_visible_child_name(if files.is_empty() {
                    "empty"
                } else {
                    "list"
                });
            }
            Some("trackers") => {
                let trackers =
                    crate::runtime::run(async move { client.trackers(&hash).await }).await?;
                if *imp.trackers.borrow() != trackers {
                    self.show_trackers(&trackers);
                    imp.trackers.replace(trackers);
                }
            }
            Some("peers") => {
                let rid = imp.peers_rid.get();
                let data =
                    crate::runtime::run(async move { client.torrent_peers(&hash, rid).await })
                        .await?;
                imp.peers_rid.set(data.rid);
                let peers = imp.peers.get().expect("set in new()");
                peers.apply(&data);
                imp.peers_stack
                    .set_visible_child_name(if peers.model().n_items() == 0 {
                        "empty"
                    } else {
                        "list"
                    });
            }
            _ => {
                let properties =
                    crate::runtime::run(async move { client.properties(&hash).await }).await?;
                imp.properties.replace(Some(properties));
                self.update_general();
            }
        }
        Ok(())
    }

    fn show_trackers(&self, trackers: &[Tracker]) {
        let group = &self.imp().trackers_group;
        // Few rows, rarely changing: rebuilding is simplest
        for row in self.imp().tracker_rows.take() {
            group.remove(&row);
        }
        for tracker in trackers {
            let title = match tracker.pseudo_source() {
                Some("DHT") => gettext("Distributed Hash Table (DHT)"),
                Some("PeX") => gettext("Peer Exchange (PeX)"),
                Some("LSD") => gettext("Local Peer Discovery (LSD)"),
                _ => tracker.url.clone(),
            };
            let mut details = vec![tracker_status_label(tracker.status())];
            if tracker.num_seeds >= 0 {
                let n = tracker.num_seeds;
                // Translators: {} is a number of seeds
                details.push(
                    ngettext("{} seed", "{} seeds", n.clamp(0, u32::MAX.into()) as u32)
                        .replace("{}", &n.to_string()),
                );
            }
            if tracker.num_peers >= 0 {
                let n = tracker.num_peers;
                // Translators: {} is a number of peers
                details.push(
                    ngettext("{} peer", "{} peers", n.clamp(0, u32::MAX.into()) as u32)
                        .replace("{}", &n.to_string()),
                );
            }
            if !tracker.msg.is_empty() {
                details.push(tracker.msg.clone());
            }
            let row = adw::ActionRow::builder()
                .title(title)
                .subtitle(details.join(" · "))
                .use_markup(false)
                .title_selectable(tracker.pseudo_source().is_none())
                .subtitle_lines(3)
                .build();
            if matches!(
                tracker.status(),
                TrackerStatus::NotWorking
                    | TrackerStatus::TrackerError
                    | TrackerStatus::Unreachable
            ) {
                let icon = gtk::Image::from_icon_name("dialog-warning-symbolic");
                icon.add_css_class("warning");
                icon.set_tooltip_text(Some(&tracker_status_label(tracker.status())));
                row.add_suffix(&icon);
            }
            group.add(&row);
            self.imp().tracker_rows.borrow_mut().push(row);
        }
    }

    /// The torrent was deleted (maybe from another client): go back to the list.
    fn close_removed(&self) {
        // Translators: {} is a torrent name
        self.show_toast(
            &gettext("“{}” is no longer on the server").replace("{}", &self.torrent().name()),
        );
        if let Some(view) = self
            .ancestor(adw::NavigationView::static_type())
            .and_downcast::<adw::NavigationView>()
        {
            view.pop_to_tag("list");
        }
    }
}

fn tracker_status_label(status: TrackerStatus) -> String {
    match status {
        TrackerStatus::Disabled => gettext("Disabled"),
        TrackerStatus::NotContacted => gettext("Not contacted yet"),
        TrackerStatus::Working => gettext("Working"),
        TrackerStatus::Updating => gettext("Updating"),
        TrackerStatus::NotWorking => gettext("Not working"),
        TrackerStatus::TrackerError => gettext("Tracker error"),
        TrackerStatus::Unreachable => gettext("Unreachable"),
    }
}

fn speed_with_average(speed: i64, average: Option<i64>) -> String {
    match average {
        // Translators: {speed} is the current speed, {average} the average, e.g. "1.2 MB/s (average 900 kB/s)"
        Some(average) if average > 0 => gettext("{speed} (average {average})")
            .replace("{speed}", &utils::format_speed(speed))
            .replace("{average}", &utils::format_speed(average)),
        _ => utils::format_speed(speed),
    }
}

fn connected_of_total(connected: i64, total: i64) -> String {
    // Translators: {connected} and {total} are numbers of seeds or peers
    gettext("{connected} connected of {total}")
        .replace("{connected}", &connected.to_string())
        .replace("{total}", &total.max(0).to_string())
}

fn format_ratio(ratio: f64) -> String {
    // qBittorrent reports "infinite" ratios as very large numbers
    if ratio >= 9999.0 {
        "∞".to_owned()
    } else {
        format!("{ratio:.2}")
    }
}
