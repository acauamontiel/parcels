//! List rows of the torrent page: files (with a priority selector) and peers.

use std::cell::{Cell, OnceCell, RefCell};
use std::sync::OnceLock;

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::glib::subclass::Signal;
use gtk::prelude::*;

use crate::model::{FileItem, Peer, Priority};
use crate::utils;

/// Order of the priority dropdown.
const PRIORITIES: [Priority; 4] = [
    Priority::Skip,
    Priority::Normal,
    Priority::High,
    Priority::Maximum,
];

fn label(classes: &[&str]) -> gtk::Label {
    gtk::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::Middle)
        .css_classes(classes.iter().map(|c| c.to_string()).collect::<Vec<_>>())
        .build()
}

mod file_imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct FileRow {
        pub name: OnceCell<gtk::Label>,
        pub folder: OnceCell<gtk::Label>,
        pub progress: OnceCell<gtk::ProgressBar>,
        pub details: OnceCell<gtk::Label>,
        pub priority: OnceCell<gtk::DropDown>,
        pub binding: RefCell<Option<(FileItem, glib::SignalHandlerId)>>,
        /// Set while the dropdown is updated from the model, not by the user
        pub updating: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for FileRow {
        const NAME: &'static str = "ParcelsFileRow";
        type Type = super::FileRow;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for FileRow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_spacing(12);
            obj.set_margin_top(9);
            obj.set_margin_bottom(9);
            obj.set_margin_start(12);
            obj.set_margin_end(12);

            let name = label(&["heading"]);
            let folder = label(&["caption", "dim-label"]);
            let progress = gtk::ProgressBar::builder().margin_top(3).build();
            let details = label(&["caption", "dim-label", "numeric"]);
            let text = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(3)
                .hexpand(true)
                .valign(gtk::Align::Center)
                .build();
            text.append(&name);
            text.append(&folder);
            text.append(&progress);
            text.append(&details);

            let names: Vec<String> = PRIORITIES.iter().map(|p| priority_label(*p)).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            let priority = gtk::DropDown::from_strings(&names);
            priority.set_valign(gtk::Align::Center);
            priority.set_tooltip_text(Some(&gettext("Download Priority")));
            priority.connect_selected_notify(glib::clone!(
                #[weak]
                obj,
                move |dropdown| {
                    if obj.imp().updating.get() {
                        return;
                    }
                    if let Some(priority) = PRIORITIES.get(dropdown.selected() as usize) {
                        obj.emit_by_name::<()>("priority-changed", &[priority]);
                    }
                }
            ));

            obj.append(&text);
            obj.append(&priority);
            self.name.set(name).expect("set once");
            self.folder.set(folder).expect("set once");
            self.progress.set(progress).expect("set once");
            self.details.set(details).expect("set once");
            self.priority.set(priority).expect("set once");
        }

        fn dispose(&self) {
            self.obj().set_file(None);
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("priority-changed")
                        .param_types([Priority::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for FileRow {}
    impl BoxImpl for FileRow {}
}

glib::wrapper! {
    pub struct FileRow(ObjectSubclass<file_imp::FileRow>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for FileRow {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl FileRow {
    pub fn file(&self) -> Option<FileItem> {
        self.imp()
            .binding
            .borrow()
            .as_ref()
            .map(|(file, _)| file.clone())
    }

    pub fn set_file(&self, file: Option<&FileItem>) {
        let imp = self.imp();
        if let Some((old, handler)) = imp.binding.take() {
            old.disconnect(handler);
        }
        let Some(file) = file else {
            return;
        };
        let handler = file.connect_notify_local(
            None,
            glib::clone!(
                #[weak(rename_to = row)]
                self,
                move |file, _| row.update(file)
            ),
        );
        imp.binding.replace(Some((file.clone(), handler)));
        self.update(file);
    }

    /// Emitted when the user picks a priority.
    pub fn connect_priority_changed<F: Fn(&Self, Priority) + 'static>(&self, f: F) {
        self.connect_local("priority-changed", false, move |args| {
            let row: Self = args[0].get().expect("signal emitter");
            f(&row, args[1].get().expect("priority"));
            None
        });
    }

    fn update(&self, file: &FileItem) {
        let imp = self.imp();
        let name = file.name();
        imp.name.get().expect("built").set_label(&name);
        imp.name
            .get()
            .expect("built")
            .set_tooltip_text(Some(&file.path()));
        let folder = file.folder();
        imp.folder.get().expect("built").set_label(&folder);
        imp.folder
            .get()
            .expect("built")
            .set_visible(!folder.is_empty());

        let progress = file.progress().clamp(0.0, 1.0);
        imp.progress.get().expect("built").set_fraction(progress);
        let details = if progress >= 1.0 {
            utils::format_size(file.size())
        } else {
            // Translators: {done} and {total} are sizes, {percent} a number, e.g. "1.2 GB of 4.5 GB (27%)"
            gettext("{done} of {total} ({percent}%)")
                .replace(
                    "{done}",
                    &utils::format_size((file.size() as f64 * progress) as i64),
                )
                .replace("{total}", &utils::format_size(file.size()))
                .replace(
                    "{percent}",
                    &((progress * 100.0).floor() as i64).to_string(),
                )
        };
        imp.details.get().expect("built").set_label(&details);

        let dropdown = imp.priority.get().expect("built");
        let index = PRIORITIES
            .iter()
            .position(|p| *p == file.priority())
            .unwrap_or(1);
        imp.updating.set(true);
        dropdown.set_selected(index as u32);
        imp.updating.set(false);
        dropdown.update_property(&[gtk::accessible::Property::Label(
            // Translators: Accessible label of the priority selector; {} is a file name
            &gettext("Priority of {}").replace("{}", &name),
        )]);
        // Skipped files look disabled, but stay usable to raise the priority again
        if file.priority() == Priority::Skip {
            imp.name.get().expect("built").add_css_class("dim-label");
        } else {
            imp.name.get().expect("built").remove_css_class("dim-label");
        }
    }
}

pub fn priority_label(priority: Priority) -> String {
    match priority {
        Priority::Skip => gettext("Don't Download"),
        Priority::Normal => gettext("Normal"),
        Priority::High => gettext("High"),
        Priority::Maximum => gettext("Maximum"),
    }
}

mod peer_imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct PeerRow {
        pub address: OnceCell<gtk::Label>,
        pub client: OnceCell<gtk::Label>,
        pub progress: OnceCell<gtk::ProgressBar>,
        pub details: OnceCell<gtk::Label>,
        pub speed: OnceCell<gtk::Label>,
        pub binding: RefCell<Option<(Peer, glib::SignalHandlerId)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PeerRow {
        const NAME: &'static str = "ParcelsPeerRow";
        type Type = super::PeerRow;
        type ParentType = gtk::Box;
    }

    impl ObjectImpl for PeerRow {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_orientation(gtk::Orientation::Vertical);
            obj.set_spacing(6);
            obj.set_margin_top(9);
            obj.set_margin_bottom(9);
            obj.set_margin_start(12);
            obj.set_margin_end(12);

            let address = label(&["heading", "numeric"]);
            address.set_hexpand(true);
            address.set_selectable(true);
            let client = label(&["caption", "dim-label"]);
            client.set_xalign(1.0);
            let top = gtk::Box::builder().spacing(12).build();
            top.append(&address);
            top.append(&client);

            let progress = gtk::ProgressBar::new();

            let details = label(&["caption", "dim-label"]);
            details.set_hexpand(true);
            let speed = label(&["caption", "dim-label", "numeric"]);
            speed.set_xalign(1.0);
            let bottom = gtk::Box::builder().spacing(12).build();
            bottom.append(&details);
            bottom.append(&speed);

            obj.append(&top);
            obj.append(&progress);
            obj.append(&bottom);
            self.address.set(address).expect("set once");
            self.client.set(client).expect("set once");
            self.progress.set(progress).expect("set once");
            self.details.set(details).expect("set once");
            self.speed.set(speed).expect("set once");
        }

        fn dispose(&self) {
            self.obj().set_peer(None);
        }
    }

    impl WidgetImpl for PeerRow {}
    impl BoxImpl for PeerRow {}
}

glib::wrapper! {
    pub struct PeerRow(ObjectSubclass<peer_imp::PeerRow>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for PeerRow {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PeerRow {
    pub fn set_peer(&self, peer: Option<&Peer>) {
        let imp = self.imp();
        if let Some((old, handler)) = imp.binding.take() {
            old.disconnect(handler);
        }
        let Some(peer) = peer else {
            return;
        };
        let handler = peer.connect_notify_local(
            None,
            glib::clone!(
                #[weak(rename_to = row)]
                self,
                move |peer, _| row.update(peer)
            ),
        );
        imp.binding.replace(Some((peer.clone(), handler)));
        self.update(peer);
    }

    fn update(&self, peer: &Peer) {
        let imp = self.imp();
        let address = match peer.host_name() {
            host if host.is_empty() => peer.address(),
            host => format!("{host} ({})", peer.address()),
        };
        imp.address.get().expect("built").set_label(&address);
        imp.client.get().expect("built").set_label(&peer.client());
        imp.progress
            .get()
            .expect("built")
            .set_fraction(peer.progress().clamp(0.0, 1.0));

        let mut details = vec![format!("{}%", (peer.progress() * 100.0).floor() as i64)];
        details.extend(
            [peer.connection(), peer.country()]
                .into_iter()
                .filter(|part| !part.is_empty()),
        );
        let details_label = imp.details.get().expect("built");
        details_label.set_label(&details.join(" · "));
        let flags = peer.flags_description();
        details_label.set_tooltip_text((!flags.is_empty()).then_some(flags.as_str()));

        let mut speeds = Vec::new();
        if peer.download_speed() > 0 {
            speeds.push(format!("↓ {}", utils::format_speed(peer.download_speed())));
        }
        if peer.upload_speed() > 0 {
            speeds.push(format!("↑ {}", utils::format_speed(peer.upload_speed())));
        }
        imp.speed
            .get()
            .expect("built")
            .set_label(&speeds.join("  "));
    }
}
