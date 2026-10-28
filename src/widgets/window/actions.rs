//! Torrent actions: start, stop, delete, category, tags, move and rename.
//!
//! Each action works on the selected torrents, calls the API on the Tokio
//! runtime and reports the outcome with a toast, offering "Undo" when the
//! change can be reverted cheaply.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib::clone;
use gtk::{gio, glib};
use qbittorrent_client::{Client, Error};

use super::Window;
use crate::model::Torrent;
use crate::widgets::tags_dialog::{TagsDialog, union_tags};

/// Actions that need at least one selected torrent.
const SELECTION_ACTIONS: [&str; 8] = [
    "start",
    "stop",
    "delete",
    "set-category",
    "new-category",
    "edit-tags",
    "move",
    "unselect-all",
];

impl Window {
    pub(super) fn setup_torrent_actions(&self) {
        let start = gio::ActionEntry::builder("start")
            .activate(|win: &Self, _, _| win.start_selected())
            .build();
        let stop = gio::ActionEntry::builder("stop")
            .activate(|win: &Self, _, _| win.stop_selected())
            .build();
        let delete = gio::ActionEntry::builder("delete")
            .activate(|win: &Self, _, _| win.confirm_delete())
            .build();
        let set_category = gio::ActionEntry::builder("set-category")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|win: &Self, _, parameter| {
                if let Some(category) = parameter.and_then(|p| p.get::<String>()) {
                    win.set_category(&category);
                }
            })
            .build();
        let new_category = gio::ActionEntry::builder("new-category")
            .activate(|win: &Self, _, _| win.show_new_category_dialog())
            .build();
        let edit_tags = gio::ActionEntry::builder("edit-tags")
            .activate(|win: &Self, _, _| win.show_tags_dialog())
            .build();
        let move_ = gio::ActionEntry::builder("move")
            .activate(|win: &Self, _, _| win.show_move_dialog())
            .build();
        let details = gio::ActionEntry::builder("details")
            .activate(|win: &Self, _, _| {
                if let [torrent] = win.selected_torrents().as_slice() {
                    win.show_details(torrent);
                }
            })
            .build();
        let rename = gio::ActionEntry::builder("rename")
            .activate(|win: &Self, _, _| win.show_rename_dialog())
            .build();
        let select_all = gio::ActionEntry::builder("select-all")
            .activate(|win: &Self, _, _| {
                if let Some(selection) = win.imp().selection.get() {
                    selection.select_all();
                }
            })
            .build();
        let unselect_all = gio::ActionEntry::builder("unselect-all")
            .activate(|win: &Self, _, _| {
                if let Some(selection) = win.imp().selection.get() {
                    selection.unselect_all();
                }
            })
            .build();
        let alt_speed = gio::ActionEntry::builder("alt-speed-limits")
            .state(false.to_variant())
            .activate(|win: &Self, action, _| win.toggle_alt_speed_limits(action))
            .build();
        self.add_action_entries([
            start,
            stop,
            delete,
            set_category,
            new_category,
            edit_tags,
            move_,
            details,
            rename,
            select_all,
            unselect_all,
            alt_speed,
        ]);

        let torrents = self.session().torrents();
        torrents.connect_alt_speed_limits_notify(clone!(
            #[weak(rename_to = win)]
            self,
            move |torrents| {
                win.change_action_state(
                    "alt-speed-limits",
                    &torrents.alt_speed_limits().to_variant(),
                );
            }
        ));
        torrents.connect_labels_changed(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_category_items()
        ));
        self.build_torrent_menu();
        self.update_selection_actions();
    }

    /// Builds the menu of the selection bar and of the context menu.
    fn build_torrent_menu(&self) {
        let imp = self.imp();
        let menu = imp.torrent_menu.get().expect("set in setup()");

        let details = gio::Menu::new();
        details.append(Some(&gettext("_Details")), Some("win.details"));

        let state = gio::Menu::new();
        state.append(Some(&gettext("_Start")), Some("win.start"));
        state.append(Some(&gettext("S_top")), Some("win.stop"));

        let categories = gio::Menu::new();
        let none = gio::MenuItem::new(Some(&gettext("_No Category")), None);
        none.set_action_and_target_value(Some("win.set-category"), Some(&"".to_variant()));
        categories.append_item(&none);
        categories.append_section(None, &imp.category_items);
        let create = gio::Menu::new();
        create.append(Some(&gettext("New _Category…")), Some("win.new-category"));
        categories.append_section(None, &create);

        let edit = gio::Menu::new();
        edit.append_submenu(Some(&gettext("_Category")), &categories);
        edit.append(Some(&gettext("_Tags…")), Some("win.edit-tags"));
        edit.append(Some(&gettext("_Move…")), Some("win.move"));
        edit.append(Some(&gettext("_Rename…")), Some("win.rename"));

        let delete = gio::Menu::new();
        delete.append(Some(&gettext("_Delete…")), Some("win.delete"));

        menu.append_section(None, &details);
        menu.append_section(None, &state);
        menu.append_section(None, &edit);
        menu.append_section(None, &delete);
        self.update_category_items();
    }

    /// Only the category names change; rebuilding the whole menu would
    /// confuse open popover menus.
    pub(super) fn update_category_items(&self) {
        let items = &self.imp().category_items;
        items.remove_all();
        for category in self.session().torrents().categories() {
            // Category names are user data: no mnemonics
            let item = gio::MenuItem::new(Some(&category.replace('_', "__")), None);
            item.set_action_and_target_value(
                Some("win.set-category"),
                Some(&category.to_variant()),
            );
            items.append_item(&item);
        }
    }

    pub(super) fn update_selection_actions(&self) {
        let count = self.selected_torrents().len();
        for name in SELECTION_ACTIONS {
            self.set_action_enabled(name, count > 0);
        }
        self.set_action_enabled("rename", count == 1);
        self.set_action_enabled("details", count == 1);

        let imp = self.imp();
        imp.selection_bar.set_revealed(count > 0);
        // Translators: {} is a number of selected torrents
        let label =
            ngettext("{} Selected", "{} Selected", count as u32).replace("{}", &count.to_string());
        imp.selection_label.set_label(&label);
    }

    pub(super) fn selected_torrents(&self) -> Vec<Torrent> {
        let Some(selection) = self.imp().selection.get() else {
            return Vec::new();
        };
        let bitset = selection.selection();
        (0..bitset.size())
            .filter_map(|i| selection.item(bitset.nth(i as u32)))
            .filter_map(|item| item.downcast::<Torrent>().ok())
            .collect()
    }

    /// Calls the API in the background; shows a toast on failure.
    fn call<F, Fut>(&self, f: F, on_success: impl FnOnce(&Self) + 'static)
    where
        F: FnOnce(Arc<Client>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<(), Error>> + Send + 'static,
    {
        glib::spawn_future_local(clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                match win.session().call(f).await {
                    Ok(()) => on_success(&win),
                    Err(message) => win.show_toast(&message),
                }
            }
        ));
    }

    pub fn show_toast(&self, title: &str) {
        let toast = adw::Toast::builder().title(title).use_markup(false).build();
        self.imp().toast_overlay.add_toast(toast);
    }

    fn show_undo_toast(&self, title: &str, undo: impl Fn(&Self) + 'static) {
        let toast = adw::Toast::builder()
            .title(title)
            .use_markup(false)
            .button_label(gettext("_Undo"))
            .build();
        toast.connect_button_clicked(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| undo(&win)
        ));
        self.imp().toast_overlay.add_toast(toast);
    }

    fn start_selected(&self) {
        let torrents = self.selected_torrents();
        self.set_running(&torrents, true, true);
    }

    fn stop_selected(&self) {
        let torrents = self.selected_torrents();
        self.set_running(&torrents, false, true);
    }

    fn set_running(&self, torrents: &[Torrent], start: bool, undoable: bool) {
        if torrents.is_empty() {
            return;
        }
        let hashes = hashes(torrents);
        let message = if start {
            // Translators: {} is a torrent name or, in the plural, a number
            subject(
                torrents,
                &ngettext("“{}” started", "{} torrents started", count(torrents)),
            )
        } else {
            // Translators: {} is a torrent name or, in the plural, a number
            subject(
                torrents,
                &ngettext("“{}” stopped", "{} torrents stopped", count(torrents)),
            )
        };
        let torrents = torrents.to_vec();
        let request_hashes = hashes.clone();
        self.call(
            move |client| async move {
                if start {
                    client.start(&request_hashes).await
                } else {
                    client.stop(&request_hashes).await
                }
            },
            move |win| {
                if undoable {
                    win.show_undo_toast(&message, move |win| {
                        win.set_running(&torrents, !start, false);
                    });
                }
            },
        );
    }

    fn confirm_delete(&self) {
        let torrents = self.selected_torrents();
        if torrents.is_empty() {
            return;
        }

        let heading = subject(
            &torrents,
            // Translators: {} is a torrent name or, in the plural, a number
            &ngettext("Delete “{}”?", "Delete {} Torrents?", count(&torrents)),
        );
        let delete_files = gtk::CheckButton::with_mnemonic(&gettext(
            "Also delete the _downloaded files from the server",
        ));
        let dialog = adw::AlertDialog::builder()
            .heading(heading)
            .body(gettext(
                "qBittorrent will stop sharing and forget the torrents. This cannot be undone.",
            ))
            .extra_child(&delete_files)
            .default_response("cancel")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("delete", &gettext("_Delete")),
        ]);
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        // Make the stakes explicit when files are deleted too
        delete_files.connect_toggled(clone!(
            #[weak]
            dialog,
            move |check| {
                let label = if check.is_active() {
                    gettext("Delete _Files")
                } else {
                    gettext("_Delete")
                };
                dialog.set_response_label("delete", &label);
            }
        ));

        dialog.connect_response(
            Some("delete"),
            clone!(
                #[weak(rename_to = win)]
                self,
                #[weak]
                delete_files,
                move |_, _| win.delete(&torrents, delete_files.is_active())
            ),
        );
        dialog.present(Some(self));
    }

    fn delete(&self, torrents: &[Torrent], delete_files: bool) {
        let hashes = hashes(torrents);
        let message = if delete_files {
            subject(
                torrents,
                // Translators: {} is a torrent name or, in the plural, a number
                &ngettext(
                    "“{}” and its files deleted",
                    "{} torrents and their files deleted",
                    count(torrents),
                ),
            )
        } else {
            // Translators: {} is a torrent name or, in the plural, a number
            subject(
                torrents,
                &ngettext("“{}” deleted", "{} torrents deleted", count(torrents)),
            )
        };
        self.call(
            move |client| async move { client.delete(&hashes, delete_files).await },
            move |win| win.show_toast(&message),
        );
    }

    fn set_category(&self, category: &str) {
        let torrents = self.selected_torrents();
        self.apply_category(&torrents, category, true);
    }

    fn apply_category(&self, torrents: &[Torrent], category: &str, undoable: bool) {
        if torrents.is_empty() {
            return;
        }
        // Remember the old categories, grouped, to undo with one call per category
        let mut previous: BTreeMap<String, Vec<Torrent>> = BTreeMap::new();
        for torrent in torrents {
            previous
                .entry(torrent.category())
                .or_default()
                .push(torrent.clone());
        }

        let hashes = hashes(torrents);
        let category = category.to_owned();
        let message = if category.is_empty() {
            gettext("Category removed")
        } else {
            // Translators: {} is a category name
            gettext("Category changed to “{}”").replace("{}", &category)
        };
        self.call(
            move |client| async move { client.set_category(&hashes, &category).await },
            move |win| {
                if undoable {
                    win.show_undo_toast(&message, move |win| {
                        for (category, torrents) in &previous {
                            win.apply_category(torrents, category, false);
                        }
                    });
                }
            },
        );
    }

    fn show_new_category_dialog(&self) {
        let torrents = self.selected_torrents();
        if torrents.is_empty() {
            return;
        }

        let name = adw::EntryRow::builder()
            .title(gettext("Name"))
            .activates_default(true)
            .build();
        let path = adw::EntryRow::builder()
            .title(gettext("Save Path on the Server (Optional)"))
            .activates_default(true)
            .build();
        let fields = boxed_list(&[name.upcast_ref(), path.upcast_ref()]);

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("New Category"))
            .extra_child(&fields)
            .default_response("create")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("create", &gettext("C_reate")),
        ]);
        dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled("create", false);
        name.connect_changed(clone!(
            #[weak]
            dialog,
            move |name| dialog.set_response_enabled("create", !name.text().trim().is_empty())
        ));

        dialog.connect_response(
            Some("create"),
            clone!(
                #[weak(rename_to = win)]
                self,
                #[weak]
                name,
                #[weak]
                path,
                move |_, _| {
                    let category = name.text().trim().to_owned();
                    let save_path = path.text().trim().to_owned();
                    let hashes = hashes(&torrents);
                    let message =
                        // Translators: {} is a category name
                        gettext("Category changed to “{}”").replace("{}", &category);
                    win.call(
                        move |client| async move {
                            client.create_category(&category, &save_path).await?;
                            client.set_category(&hashes, &category).await
                        },
                        move |win| win.show_toast(&message),
                    );
                }
            ),
        );
        dialog.present(Some(self));
        name.grab_focus();
    }

    fn show_tags_dialog(&self) {
        let torrents = self.selected_torrents();
        if torrents.is_empty() {
            return;
        }

        let torrent_tags: Vec<Vec<String>> = torrents.iter().map(Torrent::tag_list).collect();
        let all = union_tags(&torrent_tags, &self.session().torrents().tags());
        let dialog = TagsDialog::new(&all, &torrent_tags);
        dialog.connect_applied(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, changes| win.apply_tags(&torrents, changes.add, changes.remove, true)
        ));
        dialog.present(Some(self));
    }

    fn apply_tags(
        &self,
        torrents: &[Torrent],
        add: Vec<String>,
        remove: Vec<String>,
        undoable: bool,
    ) {
        if torrents.is_empty() || (add.is_empty() && remove.is_empty()) {
            return;
        }

        // Undo per tag: only touch the torrents whose tags actually changed
        let undo: Vec<(String, Vec<Torrent>, bool)> = add
            .iter()
            .map(|tag| {
                let changed = torrents
                    .iter()
                    .filter(|t| !t.has_tag(tag))
                    .cloned()
                    .collect();
                (tag.clone(), changed, false)
            })
            .chain(remove.iter().map(|tag| {
                let changed = torrents
                    .iter()
                    .filter(|t| t.has_tag(tag))
                    .cloned()
                    .collect();
                (tag.clone(), changed, true)
            }))
            .collect();

        let hashes = hashes(torrents);
        let message = gettext("Tags changed");
        self.call(
            move |client| async move {
                if !add.is_empty() {
                    client.add_tags(&hashes, &add).await?;
                }
                if !remove.is_empty() {
                    client.remove_tags(&hashes, &remove).await?;
                }
                Ok(())
            },
            move |win| {
                if undoable {
                    win.show_undo_toast(&message, move |win| {
                        for (tag, torrents, re_add) in &undo {
                            let tag = vec![tag.clone()];
                            if *re_add {
                                win.apply_tags(torrents, tag, Vec::new(), false);
                            } else {
                                win.apply_tags(torrents, Vec::new(), tag, false);
                            }
                        }
                    });
                }
            },
        );
    }

    fn show_move_dialog(&self) {
        let torrents = self.selected_torrents();
        let Some(first) = torrents.first() else {
            return;
        };

        let location = adw::EntryRow::builder()
            .title(gettext("Folder on the Server"))
            .text(first.save_path())
            .activates_default(true)
            .build();
        let fields = boxed_list(&[location.upcast_ref()]);

        let dialog = adw::AlertDialog::builder()
            .heading(subject(
                &torrents,
                // Translators: {} is a torrent name or, in the plural, a number
                &ngettext("Move “{}”", "Move {} Torrents", count(&torrents)),
            ))
            .body(gettext(
                "qBittorrent will move the downloaded data to this folder on the server.",
            ))
            .extra_child(&fields)
            .default_response("move")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", &gettext("_Cancel")), ("move", &gettext("_Move"))]);
        dialog.set_response_appearance("move", adw::ResponseAppearance::Suggested);
        location.connect_changed(clone!(
            #[weak]
            dialog,
            move |location| {
                dialog.set_response_enabled("move", !location.text().trim().is_empty());
            }
        ));

        dialog.connect_response(
            Some("move"),
            clone!(
                #[weak(rename_to = win)]
                self,
                #[weak]
                location,
                move |_, _| {
                    let hashes = hashes(&torrents);
                    let target = location.text().trim().to_owned();
                    let message = subject(
                        &torrents,
                        // Translators: {} is a torrent name or, in the plural, a number
                        &ngettext("Moving “{}”", "Moving {} torrents", count(&torrents)),
                    );
                    win.call(
                        move |client| async move { client.set_location(&hashes, &target).await },
                        move |win| win.show_toast(&message),
                    );
                }
            ),
        );
        dialog.present(Some(self));
    }

    fn show_rename_dialog(&self) {
        let torrents = self.selected_torrents();
        let [torrent] = torrents.as_slice() else {
            return;
        };

        let name = adw::EntryRow::builder()
            .title(gettext("Name"))
            .text(torrent.name())
            .activates_default(true)
            .build();
        let fields = boxed_list(&[name.upcast_ref()]);

        let dialog = adw::AlertDialog::builder()
            .heading(gettext("Rename Torrent"))
            .body(gettext(
                "Only the name shown in qBittorrent changes, not the files.",
            ))
            .extra_child(&fields)
            .default_response("rename")
            .close_response("cancel")
            .build();
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("rename", &gettext("_Rename")),
        ]);
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_response_enabled("rename", false);
        let old_name = torrent.name();
        name.connect_changed(clone!(
            #[weak]
            dialog,
            #[strong]
            old_name,
            move |name| {
                let text = name.text();
                let text = text.trim();
                dialog.set_response_enabled("rename", !text.is_empty() && text != old_name);
            }
        ));

        let torrent = torrent.clone();
        dialog.connect_response(
            Some("rename"),
            clone!(
                #[weak(rename_to = win)]
                self,
                #[weak]
                name,
                move |_, _| win.rename(&torrent, name.text().trim(), true)
            ),
        );
        dialog.present(Some(self));
    }

    fn rename(&self, torrent: &Torrent, new_name: &str, undoable: bool) {
        let hash = torrent.hash();
        let old_name = torrent.name();
        let request_name = new_name.to_owned();
        // Translators: {} is the new torrent name
        let message = gettext("Renamed to “{}”").replace("{}", new_name);
        let torrent = torrent.clone();
        self.call(
            move |client| async move { client.rename(&hash, &request_name).await },
            move |win| {
                if undoable {
                    win.show_undo_toast(&message, move |win| {
                        win.rename(&torrent, &old_name, false)
                    });
                }
            },
        );
    }

    fn toggle_alt_speed_limits(&self, action: &gio::SimpleAction) {
        // Flip right away; the next refresh confirms the server's state
        let enabled = !action
            .state()
            .and_then(|s| s.get::<bool>())
            .unwrap_or(false);
        action.set_state(&enabled.to_variant());
        glib::spawn_future_local(clone!(
            #[weak(rename_to = win)]
            self,
            #[weak]
            action,
            async move {
                let result = win
                    .session()
                    .call(|client| async move { client.toggle_alternative_speed_limits().await })
                    .await;
                if let Err(message) = result {
                    action.set_state(&(!enabled).to_variant());
                    win.show_toast(&message);
                }
            }
        ));
    }
}

fn hashes(torrents: &[Torrent]) -> Vec<String> {
    torrents.iter().map(Torrent::hash).collect()
}

fn count(torrents: &[Torrent]) -> u32 {
    torrents.len().try_into().unwrap_or(u32::MAX)
}

/// Fills a translated message with the torrent name, or the count when plural.
fn subject(torrents: &[Torrent], template: &str) -> String {
    match torrents {
        [torrent] => template.replace("{}", &torrent.name()),
        _ => template.replace("{}", &torrents.len().to_string()),
    }
}

/// Wraps rows in a boxed list, the usual look for fields in dialogs.
fn boxed_list(rows: &[&gtk::Widget]) -> gtk::ListBox {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .build();
    for row in rows {
        list.append(*row);
    }
    list
}
