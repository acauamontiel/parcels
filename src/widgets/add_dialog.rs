use std::cell::{Cell, OnceCell, RefCell};
use std::sync::{Arc, OnceLock};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib::clone;
use gtk::glib::subclass::Signal;
use gtk::{gio, glib};
use qbittorrent_client::{AddTorrents, Client, ContentLayout, TorrentFileUpload};

use crate::model::{Server, ServerManager, TorrentSource};
use crate::runtime;
use crate::session::{self, ConnectionState, Session};

/// The server torrents will be added to, ready to use.
#[derive(Debug, Clone)]
struct Target {
    client: Arc<Client>,
    /// Category name and save path
    categories: Vec<(String, String)>,
    default_save_path: String,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/add-dialog.ui")]
    pub struct AddDialog {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub add_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub sources_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub url_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub server_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub category_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub save_path_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub tags_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub start_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub layout_row: TemplateChild<adw::ComboRow>,

        pub manager: OnceCell<ServerManager>,
        pub session: OnceCell<Session>,
        pub servers: RefCell<Vec<Server>>,
        pub(super) sources: RefCell<Vec<(TorrentSource, adw::ActionRow)>>,
        pub(super) target: RefCell<Option<Target>>,
        /// Incremented on server changes, so late results are ignored
        pub attempt: Cell<u64>,
        /// The user typed a save path: stop following the category's path
        pub path_edited: Cell<bool>,
        pub setting_path: Cell<bool>,
        pub adding: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AddDialog {
        const NAME: &'static str = "ParcelsAddDialog";
        type Type = super::AddDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AddDialog {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("added")
                        .param_types([u32::static_type()])
                        .build(),
                ]
            })
        }
    }

    impl WidgetImpl for AddDialog {}
    impl AdwDialogImpl for AddDialog {}
}

glib::wrapper! {
    /// Adds magnet links, URLs and `.torrent` files to a server.
    pub struct AddDialog(ObjectSubclass<imp::AddDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl AddDialog {
    pub fn new(manager: &ServerManager, session: &Session) -> Self {
        let dialog: Self = glib::Object::new();
        let imp = dialog.imp();
        imp.manager.set(manager.clone()).expect("set once");
        imp.session.set(session.clone()).expect("set once");

        // Servers, with the active one selected
        let servers: Vec<Server> = manager
            .servers()
            .iter::<Server>()
            .filter_map(Result::ok)
            .collect();
        let names: Vec<String> = servers.iter().map(Server::name).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        imp.server_row
            .set_model(Some(&gtk::StringList::new(&names)));
        imp.server_row.set_visible(servers.len() > 1);
        let active = manager
            .active_server()
            .and_then(|active| servers.iter().position(|s| *s == active))
            .unwrap_or(0);
        imp.servers.replace(servers);
        if active == 0 {
            // Setting the same index doesn't notify
            dialog.on_server_changed();
        } else {
            imp.server_row.set_selected(active as u32);
        }

        dialog
    }

    pub fn connect_added<F: Fn(&Self, u32) + 'static>(&self, f: F) {
        self.connect_local("added", false, move |args| {
            let dialog: Self = args[0].get().expect("signal emitter");
            f(&dialog, args[1].get().expect("count"));
            None
        });
    }

    /// Adds sources to the list, skipping duplicates.
    pub fn add_sources(&self, sources: Vec<TorrentSource>) {
        let imp = self.imp();
        for source in sources {
            if imp.sources.borrow().iter().any(|(s, _)| *s == source) {
                continue;
            }

            let row = adw::ActionRow::builder()
                .title(source.title())
                .subtitle(source.subtitle())
                .use_markup(false)
                .title_lines(2)
                .build();
            if !source.is_valid() {
                row.add_css_class("error");
            }
            let remove = gtk::Button::builder()
                .icon_name("edit-delete-symbolic")
                .tooltip_text(gettext("Remove"))
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            remove.connect_clicked(clone!(
                #[weak(rename_to = dialog)]
                self,
                #[weak]
                row,
                move |_| dialog.remove_source(&row)
            ));
            row.add_suffix(&remove);

            imp.sources_group.add(&row);
            imp.sources.borrow_mut().push((source, row));
        }
        self.update_state();
    }

    fn remove_source(&self, row: &adw::ActionRow) {
        let imp = self.imp();
        imp.sources_group.remove(row);
        imp.sources.borrow_mut().retain(|(_, r)| r != row);
        self.update_state();
    }

    fn update_state(&self) {
        let imp = self.imp();
        let sources = imp.sources.borrow();
        imp.sources_group.set_visible(!sources.is_empty());
        let ready = imp.target.borrow().is_some() && !imp.adding.get();
        let valid = !sources.is_empty() && sources.iter().all(|(s, _)| s.is_valid());
        imp.add_button.set_sensitive(ready && valid);
    }

    fn show_toast(&self, message: &str) {
        let toast = adw::Toast::builder()
            .title(message)
            .use_markup(false)
            .build();
        self.imp().toast_overlay.add_toast(toast);
    }

    fn selected_server(&self) -> Option<Server> {
        let imp = self.imp();
        imp.servers
            .borrow()
            .get(imp.server_row.selected() as usize)
            .cloned()
    }

    #[template_callback]
    fn on_server_changed(&self) {
        let Some(server) = self.selected_server() else {
            return;
        };
        let imp = self.imp();
        let attempt = imp.attempt.get() + 1;
        imp.attempt.set(attempt);
        imp.target.replace(None);
        imp.category_row.set_sensitive(false);
        self.update_state();

        let session = imp.session.get().expect("set in new()").clone();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let result = load_target(&session, &server).await;
                if dialog.imp().attempt.get() != attempt {
                    return;
                }
                match result {
                    Ok(target) => dialog.set_target(target),
                    Err(message) => dialog.show_toast(&message),
                }
            }
        ));
    }

    fn set_target(&self, target: Target) {
        let imp = self.imp();
        let previous = self.selected_category();

        let mut names = vec![gettext("No Category")];
        names.extend(target.categories.iter().map(|(name, _)| name.clone()));
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        imp.category_row
            .set_model(Some(&gtk::StringList::new(&names)));
        let index = target
            .categories
            .iter()
            .position(|(name, _)| Some(name) == previous.as_ref())
            .map_or(0, |i| i + 1);

        imp.target.replace(Some(target));
        imp.category_row.set_selected(index as u32);
        imp.category_row.set_sensitive(true);
        self.on_category_changed();
        self.update_state();
    }

    fn selected_category(&self) -> Option<String> {
        let imp = self.imp();
        let index = imp.category_row.selected() as usize;
        let target = imp.target.borrow();
        let target = target.as_ref()?;
        index
            .checked_sub(1)
            .and_then(|i| target.categories.get(i))
            .map(|(name, _)| name.clone())
    }

    /// The folder the server would use: the category's, or the default one.
    fn suggested_path(&self) -> String {
        let imp = self.imp();
        let target = imp.target.borrow();
        let Some(target) = target.as_ref() else {
            return String::new();
        };
        let category = self.selected_category();
        target
            .categories
            .iter()
            .find(|(name, path)| Some(name) == category.as_ref() && !path.is_empty())
            .map(|(_, path)| path.clone())
            .unwrap_or_else(|| target.default_save_path.clone())
    }

    #[template_callback]
    fn on_category_changed(&self) {
        let imp = self.imp();
        if imp.path_edited.get() {
            return;
        }
        imp.setting_path.set(true);
        imp.save_path_row.set_text(&self.suggested_path());
        imp.setting_path.set(false);
    }

    #[template_callback]
    fn on_save_path_changed(&self) {
        let imp = self.imp();
        if !imp.setting_path.get() {
            imp.path_edited.set(true);
        }
    }

    #[template_callback]
    fn on_url_apply(&self) {
        let imp = self.imp();
        let sources = TorrentSource::from_text(&imp.url_row.text());
        if sources.is_empty() {
            self.show_toast(&gettext(
                "Enter a magnet link or the address of a torrent file",
            ));
            return;
        }
        imp.url_row.set_text("");
        self.add_sources(sources);
    }

    #[template_callback]
    fn on_open_files(&self) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some(&gettext("Torrent Files")));
        filter.add_mime_type("application/x-bittorrent");
        filter.add_suffix("torrent");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);

        let chooser = gtk::FileDialog::builder()
            .title(gettext("Open Torrent Files"))
            .filters(&filters)
            .default_filter(&filter)
            .modal(true)
            .build();
        let root = self.root().and_downcast::<gtk::Window>();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let Ok(files) = chooser.open_multiple_future(root.as_ref()).await else {
                    return; // Cancelled
                };
                let files: Vec<gio::File> =
                    files.iter::<gio::File>().filter_map(Result::ok).collect();
                let (sources, errors) = load_sources(&files).await;
                for error in errors {
                    dialog.show_toast(&error);
                }
                dialog.add_sources(sources);
            }
        ));
    }

    #[template_callback]
    fn on_cancel_clicked(&self) {
        self.close();
    }

    #[template_callback]
    fn on_add_clicked(&self) {
        let imp = self.imp();
        let Some(target) = imp.target.borrow().clone() else {
            return;
        };

        let mut torrents = AddTorrents {
            category: self.selected_category(),
            tags: imp
                .tags_row
                .text()
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect(),
            stopped: !imp.start_row.is_active(),
            content_layout: match imp.layout_row.selected() {
                1 => Some(ContentLayout::Original),
                2 => Some(ContentLayout::Subfolder),
                3 => Some(ContentLayout::NoSubfolder),
                _ => None,
            },
            ..Default::default()
        };
        // Only send a path the user chose, so qBittorrent's own rules
        // (categories, automatic management) apply otherwise
        let path = imp.save_path_row.text().trim().to_owned();
        if imp.path_edited.get() && !path.is_empty() && path != self.suggested_path() {
            torrents.save_path = Some(path);
        }
        for (source, _) in imp.sources.borrow().iter() {
            match source {
                TorrentSource::Url(url) => torrents.urls.push(url.clone()),
                TorrentSource::File(file) => torrents.files.push(file.clone()),
            }
        }
        let count = imp.sources.borrow().len() as u32;

        imp.adding.set(true);
        self.update_state();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let client = target.client.clone();
                let result = runtime::run(async move { client.add(&torrents).await }).await;
                let imp = dialog.imp();
                imp.adding.set(false);
                match result {
                    Ok(()) => {
                        if let Some(session) = imp.session.get() {
                            session.refresh_now();
                        }
                        dialog.emit_by_name::<()>("added", &[&count]);
                        dialog.close();
                    }
                    Err(err) => {
                        let auth_method = dialog
                            .selected_server()
                            .map(|s| s.auth_method())
                            .unwrap_or_default();
                        dialog.show_toast(&session::error_message(&err, auth_method));
                        dialog.update_state();
                    }
                }
            }
        ));
    }
}

/// Connects to `server` (reusing the session when it's the active one) and
/// fetches what the dialog needs: categories and the default save path.
async fn load_target(session: &Session, server: &Server) -> Result<Target, String> {
    let active =
        session.server().as_ref() == Some(server) && session.state() == ConnectionState::Connected;

    if active && let Some(client) = session.client() {
        let torrents = session.torrents();
        let categories = torrents
            .categories()
            .into_iter()
            .map(|name| {
                let path = torrents.category_save_path(&name).unwrap_or_default();
                (name, path)
            })
            .collect();
        let path_client = client.clone();
        let default_save_path = runtime::run(async move { path_client.default_save_path().await })
            .await
            .unwrap_or_default();
        return Ok(Target {
            client,
            categories,
            default_save_path,
        });
    }

    let (client, _) = session::connect_server(server).await?;
    let fetch_client = client.clone();
    let auth_method = server.auth_method();
    let (data, default_save_path) = runtime::run(async move {
        let data = fetch_client.sync_maindata(0).await?;
        let path = fetch_client.default_save_path().await.unwrap_or_default();
        Ok::<_, qbittorrent_client::Error>((data, path))
    })
    .await
    .map_err(|err| session::error_message(&err, auth_method))?;

    let mut categories: Vec<(String, String)> = data
        .categories
        .into_iter()
        .map(|(name, category)| (name, category.save_path.unwrap_or_default()))
        .collect();
    categories.sort();
    Ok(Target {
        client,
        categories,
        default_save_path,
    })
}

/// Reads a `.torrent` file chosen by the user or dropped on the window.
pub async fn read_torrent_file(file: &gio::File) -> Result<TorrentFileUpload, String> {
    let name = file
        .basename()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file.torrent".to_owned());
    let (data, _) = file.load_contents_future().await.map_err(|err| {
        // Translators: {name} is a file name, {error} a system error message
        gettext("Could not read “{name}”: {error}")
            .replace("{name}", &name)
            .replace("{error}", err.message())
    })?;
    if data.len() > crate::model::MAX_TORRENT_FILE_SIZE {
        // Translators: {} is a file name
        return Err(gettext("“{}” is too large to be a torrent file").replace("{}", &name));
    }
    Ok(TorrentFileUpload {
        file_name: name,
        data: data.to_vec(),
    })
}

/// Turns opened, dropped or pasted files into sources. Magnet links arrive as
/// `magnet:` URIs; everything else is read as a `.torrent` file.
pub async fn load_sources(files: &[gio::File]) -> (Vec<TorrentSource>, Vec<String>) {
    let mut sources = Vec::new();
    let mut errors = Vec::new();
    for file in files {
        let uri = file.uri();
        let from_uri = TorrentSource::from_text(&uri);
        if !from_uri.is_empty() {
            sources.extend(from_uri);
            continue;
        }
        match read_torrent_file(file).await {
            Ok(upload) => sources.push(TorrentSource::File(upload)),
            Err(error) => errors.push(error),
        }
    }
    (sources, errors)
}
