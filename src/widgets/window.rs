use std::cell::{OnceCell, RefCell};
use std::cmp::Ordering;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::{gettext, ngettext};
use gtk::glib::clone;
use gtk::{gio, glib};

use crate::application::Application;
use crate::config::{APP_ID, PROFILE};
use crate::model::TorrentSource;
use crate::model::{Server, ServerManager, Torrent, matches_search};
use crate::session::{ConnectionState, Session};
use crate::utils;
use crate::widgets::add_dialog::load_sources;
use crate::widgets::{AddDialog, ServerDialog, Sidebar, TorrentPage, TorrentRow};

mod actions;

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub drop_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub status_server_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub status_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub status_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub no_servers_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub failed_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub sidebar_server_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub sidebar_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub sidebar: TemplateChild<Sidebar>,
        #[template_child]
        pub download_speed_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub upload_speed_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub content_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub content_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub content_nav: TemplateChild<adw::NavigationView>,
        #[template_child]
        pub list_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub search_bar: TemplateChild<gtk::SearchBar>,
        #[template_child]
        pub search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub selection_bar: TemplateChild<gtk::ActionBar>,
        #[template_child]
        pub selection_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub selection_menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub list_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub torrent_view: TemplateChild<gtk::ListView>,
        #[template_child]
        pub empty_page: TemplateChild<adw::StatusPage>,

        pub settings: gio::Settings,
        pub manager: OnceCell<ServerManager>,
        pub session: OnceCell<Session>,
        pub filter: OnceCell<gtk::CustomFilter>,
        pub sorter: OnceCell<gtk::CustomSorter>,
        pub filter_model: OnceCell<gtk::FilterListModel>,
        pub selection: OnceCell<gtk::MultiSelection>,
        /// Actions on the selected torrents, for the selection bar and context menu
        pub torrent_menu: OnceCell<gio::Menu>,
        pub category_items: gio::Menu,
        /// The add dialog, while open: new sources go to it
        pub add_dialog: glib::WeakRef<AddDialog>,
        pub search: RefCell<String>,
        pub quitting: std::cell::Cell<bool>,
    }

    impl Default for Window {
        fn default() -> Self {
            Self {
                toast_overlay: Default::default(),
                drop_revealer: Default::default(),
                stack: Default::default(),
                status_server_button: Default::default(),
                status_title: Default::default(),
                status_stack: Default::default(),
                no_servers_page: Default::default(),
                failed_page: Default::default(),
                split_view: Default::default(),
                sidebar_server_button: Default::default(),
                sidebar_title: Default::default(),
                sidebar: Default::default(),
                download_speed_label: Default::default(),
                upload_speed_label: Default::default(),
                content_page: Default::default(),
                content_title: Default::default(),
                content_nav: Default::default(),
                list_page: Default::default(),
                search_bar: Default::default(),
                search_entry: Default::default(),
                banner: Default::default(),
                selection_bar: Default::default(),
                selection_label: Default::default(),
                selection_menu_button: Default::default(),
                list_stack: Default::default(),
                torrent_view: Default::default(),
                empty_page: Default::default(),
                settings: gio::Settings::new(*APP_ID),
                manager: Default::default(),
                session: Default::default(),
                filter: Default::default(),
                sorter: Default::default(),
                filter_model: Default::default(),
                selection: Default::default(),
                torrent_menu: Default::default(),
                category_items: gio::Menu::new(),
                add_dialog: Default::default(),
                search: Default::default(),
                quitting: Default::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "ParcelsWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            Sidebar::ensure_type();
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();

            // Devel profile: striped header bar, so it is never confused with a stable build
            if *PROFILE == "Devel" {
                obj.add_css_class("devel");
            }

            let app_icon = format!("{}-symbolic", *APP_ID);
            self.no_servers_page.set_icon_name(Some(&app_icon));
            self.empty_page.set_icon_name(Some(&app_icon));
            self.search_bar.set_key_capture_widget(Some(&*obj));

            obj.load_window_size();
        }
    }

    impl WidgetImpl for Window {
        fn map(&self) {
            self.parent_map();
            self.obj().update_foreground();
        }

        fn unmap(&self) {
            self.parent_unmap();
            self.obj().update_foreground();
        }
    }

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            let obj = self.obj();
            if let Err(err) = obj.save_window_size() {
                tracing::warn!("Failed to save window state: {err}");
            }

            // Keep running for notifications: hide instead of closing. The
            // session slows its refreshes down while the window is hidden.
            if self.settings.boolean("run-in-background") && !obj.is_quitting() {
                obj.set_visible(false);
                return glib::Propagation::Stop;
            }
            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionMap, gio::ActionGroup,
                    gtk::Root, gtk::Native, gtk::ShortcutManager,
                    gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl Window {
    pub fn new(app: &Application) -> Self {
        let window: Self = glib::Object::builder().property("application", app).build();
        window.setup(app.server_manager(), app.session());
        window
    }

    fn manager(&self) -> &ServerManager {
        self.imp().manager.get().expect("set in new()")
    }

    fn session(&self) -> &Session {
        self.imp().session.get().expect("set in new()")
    }

    fn setup(&self, manager: ServerManager, session: Session) {
        let imp = self.imp();
        imp.manager.set(manager.clone()).expect("set once");
        imp.session.set(session.clone()).expect("set once");

        let torrent_menu = gio::Menu::new();
        imp.selection_menu_button
            .set_menu_model(Some(&torrent_menu));
        imp.torrent_menu.set(torrent_menu).expect("set once");

        self.setup_actions();
        self.setup_torrent_list();
        self.setup_torrent_actions();
        self.setup_drop_target();
        self.setup_paste();

        // The closures below hold weak references to the window: the manager
        // and session outlive it, and must not keep a closed window alive.
        manager.servers().connect_items_changed(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _, _, _| {
                win.update_server_menu();
                win.update_view();
            }
        ));
        manager.connect_active_server_notify(clone!(
            #[weak(rename_to = win)]
            self,
            move |manager| {
                let id = manager.active_server().map(|s| s.id()).unwrap_or_default();
                win.change_action_state("active-server", &id.to_variant());
                // Details belong to the previous server
                win.imp().content_nav.pop_to_tag("list");
                win.update_view();
            }
        ));
        session.connect_state_notify(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_view()
        ));
        session.connect_reachable_notify(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_banner()
        ));

        let torrents = session.torrents();
        imp.sidebar.set_torrents(torrents);
        imp.sidebar.connect_filter_changed(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.on_filter_changed()
        ));
        imp.sidebar.connect_activated(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.imp().split_view.set_show_content(true)
        ));
        for property in ["download-speed", "upload-speed", "free-space"] {
            torrents.connect_notify_local(
                Some(property),
                clone!(
                    #[weak(rename_to = win)]
                    self,
                    move |_, _| win.update_transfer_info()
                ),
            );
        }

        self.update_server_menu();
        self.update_view();
        self.update_transfer_info();
        self.on_filter_changed();
        self.update_foreground();
    }

    fn setup_actions(&self) {
        let manager = self.manager();

        // Switching servers: a radio item per server, all bound to this action
        let active_id = manager.active_server().map(|s| s.id()).unwrap_or_default();
        let select_server = gio::ActionEntry::builder("active-server")
            .parameter_type(Some(glib::VariantTy::STRING))
            .state(active_id.to_variant())
            .activate(|win: &Self, _, parameter| {
                let Some(id) = parameter.and_then(|p| p.get::<String>()) else {
                    return;
                };
                if let Some(server) = win.manager().find(&id) {
                    win.manager().set_active_server(Some(&server));
                }
            })
            .build();
        let add_server = gio::ActionEntry::builder("add-server")
            .activate(|win: &Self, _, _| win.show_server_dialog(None))
            .build();
        let edit_server = gio::ActionEntry::builder("edit-server")
            .activate(|win: &Self, _, _| {
                let server = win.manager().active_server();
                win.show_server_dialog(server.as_ref());
            })
            .build();
        let reconnect = gio::ActionEntry::builder("reconnect")
            .activate(|win: &Self, _, _| win.session().reconnect())
            .build();
        let search = gio::ActionEntry::builder("search")
            .activate(|win: &Self, _, _| {
                let imp = win.imp();
                imp.split_view.set_show_content(true);
                imp.search_bar.set_search_mode(true);
                imp.search_entry.grab_focus();
            })
            .build();
        let add_torrent = gio::ActionEntry::builder("add-torrent")
            .activate(|win: &Self, _, _| win.show_add_dialog(Vec::new()))
            .build();
        let open_torrent = gio::ActionEntry::builder("open-torrent")
            .activate(|win: &Self, _, _| win.choose_torrent_files())
            .build();
        self.add_action_entries([
            select_server,
            add_server,
            edit_server,
            reconnect,
            search,
            add_torrent,
            open_torrent,
        ]);

        // Sorting actions are stored straight into GSettings
        let settings = &self.imp().settings;
        self.add_action(&settings.create_action("sort-by"));
        self.add_action(&settings.create_action("sort-descending"));
        for key in ["sort-by", "sort-descending"] {
            settings.connect_changed(
                Some(key),
                clone!(
                    #[weak(rename_to = win)]
                    self,
                    move |_, _| win.resort()
                ),
            );
        }
    }

    fn setup_torrent_list(&self) {
        let imp = self.imp();
        let torrents = self.session().torrents();

        let filter = gtk::CustomFilter::new(clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            false,
            move |item| {
                let Some(torrent) = item.downcast_ref::<Torrent>() else {
                    return false;
                };
                win.imp().sidebar.filter().matches(torrent)
                    && matches_search(torrent, &win.imp().search.borrow())
            }
        ));
        let sorter = gtk::CustomSorter::new(clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            gtk::Ordering::Equal,
            move |a, b| {
                match (a.downcast_ref::<Torrent>(), b.downcast_ref::<Torrent>()) {
                    (Some(a), Some(b)) => win.compare(a, b).into(),
                    _ => gtk::Ordering::Equal,
                }
            }
        ));

        let filter_model =
            gtk::FilterListModel::new(Some(torrents.model().clone()), Some(filter.clone()));
        let sort_model = gtk::SortListModel::new(Some(filter_model.clone()), Some(sorter.clone()));
        let selection = gtk::MultiSelection::new(Some(sort_model));
        selection.connect_selection_changed(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _, _| win.update_selection_actions()
        ));
        selection.connect_items_changed(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _, _, _| win.update_selection_actions()
        ));
        imp.torrent_view.set_model(Some(&selection));
        // Scoped to the list: a window-wide Delete would also fire while typing
        // in the search entry. The list view has Ctrl+A and Ctrl+Shift+A built in.
        let shortcuts = gtk::ShortcutController::new();
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("Delete|KP_Delete"),
            Some(gtk::NamedAction::new("win.delete")),
        ));
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("<Shift>F10|Menu"),
            Some(gtk::CallbackAction::new(|view, _| {
                // The focused child is the list's item widget; the row is inside it
                let row = view
                    .focus_child()
                    .and_then(|item| item.first_child())
                    .and_downcast::<TorrentRow>();
                match row {
                    Some(row) => {
                        row.open_menu_from_keyboard();
                        glib::Propagation::Stop
                    }
                    None => glib::Propagation::Proceed,
                }
            })),
        ));
        imp.torrent_view.add_controller(shortcuts);
        imp.selection.set(selection).expect("set once");

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, item| {
                let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
                let row = TorrentRow::default();
                win.setup_context_menu(&row, item);
                item.set_child(Some(&row));
            }
        ));
        factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            let row = item
                .child()
                .and_downcast::<TorrentRow>()
                .expect("a torrent row");
            row.set_torrent(item.item().and_downcast_ref::<Torrent>());
        });
        factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
            if let Some(row) = item.child().and_downcast::<TorrentRow>() {
                row.set_torrent(None);
            }
        });
        imp.torrent_view.set_factory(Some(&factory));
        // Enter or double click opens the details
        imp.torrent_view.connect_activate(clone!(
            #[weak(rename_to = win)]
            self,
            move |view, position| {
                let torrent = view
                    .model()
                    .and_then(|model| model.item(position))
                    .and_downcast::<Torrent>();
                if let Some(torrent) = torrent {
                    win.show_details(&torrent);
                }
            }
        ));

        filter_model.connect_items_changed(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, _, _, _| win.update_list_page()
        ));
        // Torrents change in place, so filters and sorting must be re-run
        torrents.connect_updated(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| {
                win.refilter();
                win.resort();
            }
        ));

        imp.filter.set(filter).expect("set once");
        imp.sorter.set(sorter).expect("set once");
        imp.filter_model.set(filter_model).expect("set once");
    }

    /// Right click or long press on a row opens the torrent menu. Like in file
    /// managers, it acts on the selection, which is the clicked row unless
    /// the row is already part of a larger selection.
    fn setup_context_menu(&self, row: &TorrentRow, item: &gtk::ListItem) {
        let open = clone!(
            #[weak(rename_to = win)]
            self,
            #[weak]
            row,
            #[weak]
            item,
            move |x: f64, y: f64| {
                let (Some(selection), Some(menu)) =
                    (win.imp().selection.get(), win.imp().torrent_menu.get())
                else {
                    return;
                };
                let position = item.position();
                if position == gtk::INVALID_LIST_POSITION {
                    return;
                }
                if !selection.is_selected(position) {
                    selection.select_item(position, true);
                }
                row.popup_menu(menu.upcast_ref(), x, y);
            }
        );

        row.set_menu_opener(open.clone());

        let click = gtk::GestureClick::builder()
            .button(gtk::gdk::BUTTON_SECONDARY)
            .build();
        click.connect_pressed(clone!(
            #[strong]
            open,
            move |gesture, _, x, y| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                open(x, y);
            }
        ));
        row.add_controller(click);

        let long_press = gtk::GestureLongPress::builder().touch_only(true).build();
        long_press.connect_pressed(move |gesture, x, y| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            open(x, y);
        });
        row.add_controller(long_press);
    }

    fn compare(&self, a: &Torrent, b: &Torrent) -> Ordering {
        let settings = &self.imp().settings;
        let ordering = match settings.string("sort-by").as_str() {
            "name" => a.name().to_lowercase().cmp(&b.name().to_lowercase()),
            "progress" => a.progress().total_cmp(&b.progress()),
            "size" => a.size().cmp(&b.size()),
            "download-speed" => a.download_speed().cmp(&b.download_speed()),
            "upload-speed" => a.upload_speed().cmp(&b.upload_speed()),
            // Unknown ETA (-1) sorts last in ascending order
            "eta" => (a.eta() < 0, a.eta()).cmp(&(b.eta() < 0, b.eta())),
            "ratio" => a.ratio().total_cmp(&b.ratio()),
            _ => a.added_on().cmp(&b.added_on()),
        };
        // Ties: by name, so the order is stable
        let ordering = ordering.then_with(|| a.name().cmp(&b.name()));
        if settings.boolean("sort-descending") {
            ordering.reverse()
        } else {
            ordering
        }
    }

    fn refilter(&self) {
        if let Some(filter) = self.imp().filter.get() {
            filter.changed(gtk::FilterChange::Different);
        }
        self.update_list_page();
    }

    fn resort(&self) {
        if let Some(sorter) = self.imp().sorter.get() {
            sorter.changed(gtk::SorterChange::Different);
        }
    }

    pub fn show_details(&self, torrent: &Torrent) {
        let imp = self.imp();
        imp.split_view.set_show_content(true);
        imp.content_nav.pop_to_tag("list");
        let page = TorrentPage::new(torrent, self.session());
        imp.content_nav.push(&page);
    }

    fn on_filter_changed(&self) {
        let imp = self.imp();
        let title = imp.sidebar.filter().title();
        imp.content_page.set_title(&title);
        imp.list_page.set_title(&title);
        imp.content_title.set_title(&title);
        self.refilter();
    }

    #[template_callback]
    fn on_search_changed(&self) {
        let text = self.imp().search_entry.text().to_string();
        self.imp().search.replace(text);
        self.refilter();
    }

    #[template_callback]
    fn on_banner_retry(&self) {
        self.session().refresh_now();
    }

    fn update_list_page(&self) {
        let imp = self.imp();
        let Some(filter_model) = imp.filter_model.get() else {
            return;
        };
        let page = if filter_model.n_items() > 0 {
            "list"
        } else if self.session().torrents().len() == 0 {
            "empty"
        } else {
            "no-results"
        };
        imp.list_stack.set_visible_child_name(page);
    }

    fn update_transfer_info(&self) {
        let imp = self.imp();
        let torrents = self.session().torrents();

        let download = utils::format_speed(torrents.download_speed());
        let upload = utils::format_speed(torrents.upload_speed());
        imp.download_speed_label.set_label(&format!("↓ {download}"));
        imp.upload_speed_label.set_label(&format!("↑ {upload}"));
        imp.download_speed_label
            .update_property(&[gtk::accessible::Property::Label(
                // Translators: {} is a speed, e.g. "1.2 MB/s"
                &gettext("Download speed: {}").replace("{}", &download),
            )]);
        imp.upload_speed_label
            .update_property(&[gtk::accessible::Property::Label(
                // Translators: {} is a speed, e.g. "1.2 MB/s"
                &gettext("Upload speed: {}").replace("{}", &upload),
            )]);

        // Translators: {} is a size, e.g. "120 GB"
        let free = gettext("Free space on the server: {}")
            .replace("{}", &utils::format_size(torrents.free_space()));
        imp.download_speed_label.set_tooltip_text(Some(&free));
        imp.upload_speed_label.set_tooltip_text(Some(&free));
    }

    fn update_banner(&self) {
        let session = self.session();
        let revealed = !session.reachable() && session.state() == ConnectionState::Connected;
        self.imp().banner.set_revealed(revealed);
    }

    /// Lets the session refresh often only while the window is on screen.
    fn update_foreground(&self) {
        if let Some(session) = self.imp().session.get() {
            session.set_foreground(self.is_mapped());
        }
    }

    fn update_server_menu(&self) {
        let servers = gio::Menu::new();
        for server in self
            .manager()
            .servers()
            .iter::<Server>()
            .filter_map(Result::ok)
        {
            let item = gio::MenuItem::new(Some(&server.name()), None);
            item.set_action_and_target_value(
                Some("win.active-server"),
                Some(&server.id().to_variant()),
            );
            servers.append_item(&item);
        }

        let actions = gio::Menu::new();
        actions.append(Some(&gettext("_Add Server…")), Some("win.add-server"));
        actions.append(Some(&gettext("_Edit Server…")), Some("win.edit-server"));

        let menu = gio::Menu::new();
        menu.append_section(None, &servers);
        menu.append_section(None, &actions);
        let imp = self.imp();
        imp.status_server_button.set_menu_model(Some(&menu));
        imp.sidebar_server_button.set_menu_model(Some(&menu));
    }

    fn update_view(&self) {
        let imp = self.imp();
        let session = self.session();
        let has_servers = self.manager().servers().n_items() > 0;

        imp.status_server_button.set_visible(has_servers);
        self.set_action_enabled("edit-server", session.server().is_some());
        self.set_action_enabled("reconnect", session.state() != ConnectionState::Connecting);

        // The active server is always visible in a title or subtitle
        let server_name = session.server().map(|s| s.name()).unwrap_or_default();
        imp.status_title.set_subtitle(&server_name);
        imp.sidebar_title.set_subtitle(&server_name);
        imp.content_title.set_subtitle(&server_name);

        let status_page = if !has_servers {
            Some("no-servers")
        } else {
            match session.state() {
                ConnectionState::Idle | ConnectionState::Connecting => Some("connecting"),
                ConnectionState::Failed => {
                    imp.failed_page
                        .set_description(Some(&glib::markup_escape_text(&session.error_message())));
                    Some("failed")
                }
                ConnectionState::Connected => None,
            }
        };
        match status_page {
            Some(page) => {
                imp.status_stack.set_visible_child_name(page);
                imp.stack.set_visible_child_name("status");
            }
            None => imp.stack.set_visible_child_name("torrents"),
        }
        self.update_banner();
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        {
            action.set_enabled(enabled);
        }
    }

    /// Opens the add dialog with `sources`, or adds them to the open one.
    pub fn show_add_dialog(&self, sources: Vec<TorrentSource>) {
        let imp = self.imp();
        if self.manager().servers().n_items() == 0 {
            self.show_toast(&gettext("Add a server first, then add the torrents again"));
            self.show_server_dialog(None);
            return;
        }

        if let Some(dialog) = imp.add_dialog.upgrade() {
            dialog.add_sources(sources);
            return;
        }

        let dialog = AddDialog::new(self.manager(), self.session());
        dialog.add_sources(sources);
        dialog.connect_added(clone!(
            #[weak(rename_to = win)]
            self,
            move |_, count| {
                // Translators: {} is a number of torrents
                let message = ngettext("Torrent added", "{} torrents added", count)
                    .replace("{}", &count.to_string());
                win.show_toast(&message);
            }
        ));
        imp.add_dialog.set(Some(&dialog));
        dialog.present(Some(self));
    }

    /// Handles files opened from the desktop: `.torrent` files and `magnet:` links.
    pub fn open_files(&self, files: Vec<gio::File>) {
        glib::spawn_future_local(clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                let (sources, errors) = load_sources(&files).await;
                for error in errors {
                    win.show_toast(&error);
                }
                if !sources.is_empty() {
                    win.show_add_dialog(sources);
                }
            }
        ));
    }

    fn choose_torrent_files(&self) {
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
        glib::spawn_future_local(clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                if let Ok(files) = chooser.open_multiple_future(Some(&win)).await {
                    let files = files.iter::<gio::File>().filter_map(Result::ok).collect();
                    win.open_files(files);
                }
            }
        ));
    }

    /// Accepts `.torrent` files and magnet links dragged onto the window.
    fn setup_drop_target(&self) {
        let target = gtk::DropTarget::new(glib::Type::INVALID, gtk::gdk::DragAction::COPY);
        target.set_types(&[gtk::gdk::FileList::static_type(), String::static_type()]);
        target.connect_drop(clone!(
            #[weak(rename_to = win)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| {
                if let Ok(files) = value.get::<gtk::gdk::FileList>() {
                    win.open_files(files.files());
                    return true;
                }
                let sources = value
                    .get::<String>()
                    .map(|text| TorrentSource::from_text(&text))
                    .unwrap_or_default();
                if sources.is_empty() {
                    return false;
                }
                win.show_add_dialog(sources);
                true
            }
        ));
        target.connect_current_drop_notify(clone!(
            #[weak(rename_to = win)]
            self,
            move |target| {
                win.imp()
                    .drop_revealer
                    .set_reveal_child(target.current_drop().is_some());
            }
        ));
        self.imp().toast_overlay.add_controller(target);
    }

    /// Ctrl+V pastes magnet links or `.torrent` files. The shortcut runs in
    /// the bubble phase, so text fields that handle Ctrl+V keep working.
    fn setup_paste(&self) {
        let shortcuts = gtk::ShortcutController::new();
        shortcuts.set_propagation_phase(gtk::PropagationPhase::Bubble);
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string("<Control>v"),
            Some(gtk::CallbackAction::new(|widget, _| {
                if let Some(win) = widget.downcast_ref::<Self>() {
                    win.paste();
                }
                glib::Propagation::Stop
            })),
        ));
        self.add_controller(shortcuts);
    }

    fn paste(&self) {
        let clipboard = self.clipboard();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = win)]
            self,
            async move {
                if clipboard
                    .formats()
                    .contains_type(gtk::gdk::FileList::static_type())
                    && let Ok(value) = clipboard
                        .read_value_future(
                            gtk::gdk::FileList::static_type(),
                            glib::Priority::DEFAULT,
                        )
                        .await
                    && let Ok(files) = value.get::<gtk::gdk::FileList>()
                {
                    win.open_files(files.files());
                    return;
                }

                let text = clipboard.read_text_future().await.ok().flatten();
                let sources = text
                    .map(|text| TorrentSource::from_text(&text))
                    .unwrap_or_default();
                if sources.is_empty() {
                    win.show_toast(&gettext("The clipboard has no magnet link or torrent file"));
                } else {
                    win.show_add_dialog(sources);
                }
            }
        ));
    }

    fn show_server_dialog(&self, server: Option<&Server>) {
        ServerDialog::new(self.manager(), server).present(Some(self));
    }

    /// Quitting closes the window for real, even when running in the background.
    pub fn quit(&self) {
        self.imp().quitting.set(true);
        self.close();
    }

    fn is_quitting(&self) -> bool {
        self.imp().quitting.get()
    }

    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = &self.imp().settings;
        let (width, height) = self.default_size();

        settings.set_int("window-width", width)?;
        settings.set_int("window-height", height)?;
        settings.set_boolean("is-maximized", self.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let settings = &self.imp().settings;

        self.set_default_size(settings.int("window-width"), settings.int("window-height"));

        if settings.boolean("is-maximized") {
            self.maximize();
        }
    }
}
