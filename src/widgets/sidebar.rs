use std::cell::{OnceCell, RefCell};
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::glib::subclass::Signal;

use crate::model::{Filter, Torrent, TorrentList};

#[derive(Debug)]
struct Entry {
    row: gtk::ListBoxRow,
    filter: Filter,
    count: gtk::Label,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct Sidebar {
        pub list_box: OnceCell<gtk::ListBox>,
        pub(super) entries: RefCell<Vec<Entry>>,
        pub filter: RefCell<Filter>,
        pub torrents: OnceCell<TorrentList>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Sidebar {
        const NAME: &'static str = "ParcelsSidebar";
        type Type = super::Sidebar;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for Sidebar {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            let list_box = gtk::ListBox::builder()
                .selection_mode(gtk::SelectionMode::Browse)
                .css_classes(["navigation-sidebar"])
                .build();
            list_box.update_property(&[gtk::accessible::Property::Label(&gettext("Filters"))]);
            // The "No Category" and "No Tags" rows start their sections
            list_box.set_header_func(glib::clone!(
                #[weak]
                obj,
                move |row, _| {
                    let header = obj
                        .imp()
                        .entries
                        .borrow()
                        .iter()
                        .find(|entry| &entry.row == row)
                        .and_then(|entry| section_title(&entry.filter));
                    row.set_header(header.map(|text| section_header(&text)).as_ref());
                }
            ));
            list_box.connect_row_selected(glib::clone!(
                #[weak]
                obj,
                move |_, row| obj.on_row_selected(row)
            ));
            // In the collapsed layout, activating a row should show the list
            // even when it is already selected
            list_box.connect_row_activated(glib::clone!(
                #[weak]
                obj,
                move |_, _| obj.emit_by_name::<()>("activated", &[])
            ));

            let scrolled = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vexpand(true)
                .child(&list_box)
                .build();
            obj.set_child(Some(&scrolled));
            self.list_box.set(list_box).expect("set once");
        }

        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("filter-changed").build(),
                    Signal::builder("activated").build(),
                ]
            })
        }
    }

    impl WidgetImpl for Sidebar {}
    impl BinImpl for Sidebar {}
}

glib::wrapper! {
    /// Status, category and tag filters, each with the number of matching torrents.
    pub struct Sidebar(ObjectSubclass<imp::Sidebar>)
        @extends gtk::Widget, adw::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Sidebar {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Sidebar {
    pub fn filter(&self) -> Filter {
        self.imp().filter.borrow().clone()
    }

    fn list_box(&self) -> &gtk::ListBox {
        self.imp().list_box.get().expect("set in constructed()")
    }

    pub fn set_torrents(&self, torrents: &TorrentList) {
        self.imp().torrents.set(torrents.clone()).expect("set once");
        torrents.connect_labels_changed(glib::clone!(
            #[weak(rename_to = sidebar)]
            self,
            move |_| sidebar.rebuild()
        ));
        torrents.connect_updated(glib::clone!(
            #[weak(rename_to = sidebar)]
            self,
            move |_| sidebar.update_counts()
        ));
        self.rebuild();
    }

    pub fn connect_filter_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_local("filter-changed", false, move |args| {
            f(&args[0].get().expect("signal emitter"));
            None
        });
    }

    pub fn connect_activated<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_local("activated", false, move |args| {
            f(&args[0].get().expect("signal emitter"));
            None
        });
    }

    fn rebuild(&self) {
        let Some(torrents) = self.imp().torrents.get() else {
            return;
        };
        let list_box = self.list_box();
        list_box.remove_all();

        let mut entries = Vec::new();
        for filter in Filter::STATUSES {
            entries.push(self.add_row(filter));
        }

        let categories = torrents.categories();
        if !categories.is_empty() {
            entries.push(self.add_row(Filter::Category(String::new())));
            for category in categories {
                entries.push(self.add_row(Filter::Category(category)));
            }
        }

        let tags = torrents.tags();
        if !tags.is_empty() {
            entries.push(self.add_row(Filter::Tag(String::new())));
            for tag in tags {
                entries.push(self.add_row(Filter::Tag(tag)));
            }
        }

        // Keep the selection; fall back to "All" if its category or tag is gone
        let current = self.filter();
        let selected = entries
            .iter()
            .find(|entry| entry.filter == current)
            .or_else(|| entries.first())
            .map(|entry| entry.row.clone());
        self.imp().entries.replace(entries);
        list_box.invalidate_headers();
        list_box.select_row(selected.as_ref());

        self.update_counts();
    }

    fn add_row(&self, filter: Filter) -> Entry {
        let icon = gtk::Image::from_icon_name(filter.icon_name());
        icon.set_accessible_role(gtk::AccessibleRole::Presentation);
        let title = gtk::Label::builder()
            .label(filter.title())
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        let count = gtk::Label::builder()
            .css_classes(["dim-label", "numeric"])
            .build();

        let content = gtk::Box::builder().spacing(12).build();
        content.append(&icon);
        content.append(&title);
        content.append(&count);

        let row = gtk::ListBoxRow::builder().child(&content).build();
        self.list_box().append(&row);

        Entry { row, filter, count }
    }

    fn update_counts(&self) {
        let Some(torrents) = self.imp().torrents.get() else {
            return;
        };
        let all: Vec<Torrent> = torrents
            .model()
            .iter::<Torrent>()
            .filter_map(Result::ok)
            .collect();

        for entry in self.imp().entries.borrow().iter() {
            let count = all.iter().filter(|t| entry.filter.matches(t)).count();
            entry.count.set_label(&count.to_string());
            // Read as "Downloading, 3"
            entry
                .row
                .update_property(&[gtk::accessible::Property::Label(&format!(
                    "{}, {count}",
                    entry.filter.title()
                ))]);
        }
    }

    fn on_row_selected(&self, row: Option<&gtk::ListBoxRow>) {
        let Some(row) = row else {
            return;
        };
        let filter = self
            .imp()
            .entries
            .borrow()
            .iter()
            .find(|entry| &entry.row == row)
            .map(|entry| entry.filter.clone());
        if let Some(filter) = filter
            && *self.imp().filter.borrow() != filter
        {
            self.imp().filter.replace(filter);
            self.emit_by_name::<()>("filter-changed", &[]);
        }
    }
}

/// The title of the section a row starts, if any.
fn section_title(filter: &Filter) -> Option<String> {
    match filter {
        Filter::Category(category) if category.is_empty() => Some(gettext("Categories")),
        Filter::Tag(tag) if tag.is_empty() => Some(gettext("Tags")),
        _ => None,
    }
}

fn section_header(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .margin_top(12)
        .margin_bottom(6)
        .margin_start(12)
        .css_classes(["heading", "dim-label"])
        .build()
}
