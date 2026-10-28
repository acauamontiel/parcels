use std::cell::RefCell;
use std::collections::BTreeSet;
use std::sync::OnceLock;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::glib::subclass::Signal;

/// Tag changes chosen in the dialog.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TagChanges {
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

#[derive(Debug)]
struct TagRow {
    tag: String,
    check: gtk::CheckButton,
    /// Some, but not all, selected torrents have the tag
    initially_mixed: bool,
    initially_active: bool,
}

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct TagsDialog {
        pub(super) rows: RefCell<Vec<TagRow>>,
        pub group: RefCell<Option<adw::PreferencesGroup>>,
        pub changes: RefCell<TagChanges>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TagsDialog {
        const NAME: &'static str = "ParcelsTagsDialog";
        type Type = super::TagsDialog;
        type ParentType = adw::Dialog;
    }

    impl ObjectImpl for TagsDialog {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| vec![Signal::builder("applied").build()])
        }
    }

    impl WidgetImpl for TagsDialog {}
    impl AdwDialogImpl for TagsDialog {}
}

glib::wrapper! {
    /// Adds and removes tags on one or more torrents.
    pub struct TagsDialog(ObjectSubclass<imp::TagsDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl TagsDialog {
    /// `all_tags` are the server's tags; `torrent_tags` the tags of each
    /// selected torrent.
    pub fn new(all_tags: &[String], torrent_tags: &[Vec<String>]) -> Self {
        let dialog: Self = glib::Object::builder()
            .property("title", gettext("Edit Tags"))
            .property("content-width", 400)
            .property("content-height", 480)
            .build();

        let cancel = gtk::Button::with_mnemonic(&gettext("_Cancel"));
        cancel.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| {
                dialog.close();
            }
        ));
        let apply = gtk::Button::builder()
            .label(gettext("_Apply"))
            .use_underline(true)
            .css_classes(["suggested-action"])
            .build();
        apply.connect_clicked(glib::clone!(
            #[weak]
            dialog,
            move |_| dialog.apply()
        ));
        dialog.set_default_widget(Some(&apply));

        let header = adw::HeaderBar::builder()
            .show_start_title_buttons(false)
            .show_end_title_buttons(false)
            .build();
        header.pack_start(&cancel);
        header.pack_end(&apply);

        let new_tag = adw::EntryRow::builder()
            .title(gettext("New Tag"))
            .show_apply_button(true)
            .build();
        new_tag.connect_apply(glib::clone!(
            #[weak]
            dialog,
            move |entry| {
                let tag = entry.text().trim().to_owned();
                // qBittorrent splits tags on commas
                if !tag.is_empty() && !tag.contains(',') {
                    dialog.add_tag_row(&tag, true, false);
                    entry.set_text("");
                }
            }
        ));
        let new_group = adw::PreferencesGroup::new();
        new_group.add(&new_tag);

        let group = adw::PreferencesGroup::builder()
            .description(gettext(
                "Tags marked with a dash are on some of the selected torrents",
            ))
            .build();
        dialog.imp().group.replace(Some(group.clone()));

        let page = adw::PreferencesPage::new();
        page.add(&new_group);
        page.add(&group);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&page));
        dialog.set_child(Some(&toolbar));

        let total = torrent_tags.len();
        for tag in all_tags {
            let count = torrent_tags
                .iter()
                .filter(|tags| tags.contains(tag))
                .count();
            dialog.add_tag_row(tag, count == total && total > 0, count > 0 && count < total);
        }
        group.set_visible(!all_tags.is_empty());

        dialog
    }

    fn add_tag_row(&self, tag: &str, active: bool, mixed: bool) {
        let imp = self.imp();
        if let Some(existing) = imp.rows.borrow().iter().find(|row| row.tag == tag) {
            existing.check.set_inconsistent(false);
            existing.check.set_active(true);
            return;
        }

        let check = gtk::CheckButton::builder()
            .active(active)
            .inconsistent(mixed)
            .valign(gtk::Align::Center)
            .build();
        // A click on a mixed check settles it
        check.connect_toggled(|check| check.set_inconsistent(false));
        let row = adw::ActionRow::builder()
            .title(tag)
            .use_markup(false)
            .activatable_widget(&check)
            .build();
        row.add_prefix(&check);

        if let Some(group) = imp.group.borrow().as_ref() {
            group.add(&row);
            group.set_visible(true);
        }
        imp.rows.borrow_mut().push(TagRow {
            tag: tag.to_owned(),
            check,
            initially_mixed: mixed,
            initially_active: active,
        });
    }

    fn apply(&self) {
        let imp = self.imp();
        let mut changes = TagChanges::default();
        for row in imp.rows.borrow().iter() {
            if row.check.is_inconsistent() {
                continue;
            }
            let active = row.check.is_active();
            if active && (!row.initially_active || row.initially_mixed) {
                changes.add.push(row.tag.clone());
            } else if !active && (row.initially_active || row.initially_mixed) {
                changes.remove.push(row.tag.clone());
            }
        }

        imp.changes.replace(changes);
        self.emit_by_name::<()>("applied", &[]);
        self.close();
    }

    pub fn connect_applied<F: Fn(&Self, TagChanges) + 'static>(&self, f: F) {
        self.connect_local("applied", false, move |args| {
            let dialog: Self = args[0].get().expect("signal emitter");
            let changes = dialog.imp().changes.borrow().clone();
            f(&dialog, changes);
            None
        });
    }
}

/// Union of tags, sorted, for building the dialog.
pub fn union_tags(lists: &[Vec<String>], server_tags: &[String]) -> Vec<String> {
    let mut all: BTreeSet<String> = server_tags.iter().cloned().collect();
    all.extend(lists.iter().flatten().cloned());
    all.into_iter().collect()
}
