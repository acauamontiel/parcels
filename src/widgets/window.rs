use std::cell::OnceCell;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib::clone;
use gtk::{gio, glib};

use crate::application::Application;
use crate::config::{APP_ID, PROFILE};
use crate::model::{Server, ServerManager};
use crate::session::{ConnectionState, Session};
use crate::widgets::ServerDialog;

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/window.ui")]
    pub struct Window {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub server_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub window_title: TemplateChild<adw::WindowTitle>,
        #[template_child]
        pub stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub no_servers_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub failed_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub connected_page: TemplateChild<adw::StatusPage>,
        pub settings: gio::Settings,
        pub manager: OnceCell<ServerManager>,
        pub session: OnceCell<Session>,
    }

    impl Default for Window {
        fn default() -> Self {
            Self {
                toast_overlay: TemplateChild::default(),
                server_button: TemplateChild::default(),
                window_title: TemplateChild::default(),
                stack: TemplateChild::default(),
                no_servers_page: TemplateChild::default(),
                failed_page: TemplateChild::default(),
                connected_page: TemplateChild::default(),
                settings: gio::Settings::new(*APP_ID),
                manager: OnceCell::new(),
                session: OnceCell::new(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "ParcelsWindow";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
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
            self.connected_page.set_icon_name(Some(&app_icon));

            obj.load_window_size();
        }
    }

    impl WidgetImpl for Window {}

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            if let Err(err) = self.obj().save_window_size() {
                tracing::warn!("Failed to save window state: {err}");
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
        self.add_action_entries([select_server, add_server, edit_server, reconnect]);

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
                win.update_view();
            }
        ));
        session.connect_state_notify(clone!(
            #[weak(rename_to = win)]
            self,
            move |_| win.update_view()
        ));

        self.update_server_menu();
        self.update_view();
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
        self.imp().server_button.set_menu_model(Some(&menu));
    }

    fn update_view(&self) {
        let imp = self.imp();
        let session = self.session();
        let has_servers = self.manager().servers().n_items() > 0;

        imp.server_button.set_visible(has_servers);
        self.set_action_enabled("edit-server", session.server().is_some());
        self.set_action_enabled("reconnect", session.state() != ConnectionState::Connecting);
        imp.window_title.set_subtitle(
            &session
                .server()
                .map(|server| server.name())
                .unwrap_or_default(),
        );

        let page = if !has_servers {
            "no-servers"
        } else {
            match session.state() {
                ConnectionState::Idle | ConnectionState::Connecting => "connecting",
                ConnectionState::Failed => {
                    imp.failed_page
                        .set_description(Some(&glib::markup_escape_text(&session.error_message())));
                    "failed"
                }
                ConnectionState::Connected => {
                    // Translators: {} is a version number, e.g. 5.1.2
                    let description =
                        gettext("qBittorrent {}").replace("{}", &session.server_version());
                    imp.connected_page.set_description(Some(&description));
                    "connected"
                }
            }
        };
        imp.stack.set_visible_child_name(page);
    }

    fn set_action_enabled(&self, name: &str, enabled: bool) {
        if let Some(action) = self
            .lookup_action(name)
            .and_then(|a| a.downcast::<gio::SimpleAction>().ok())
        {
            action.set_enabled(enabled);
        }
    }

    fn show_server_dialog(&self, server: Option<&Server>) {
        ServerDialog::new(self.manager(), server).present(Some(self));
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
