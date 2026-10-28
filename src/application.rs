use gettextrs::gettext;
use tracing::{debug, info};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::config::{APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::model::ServerManager;
use crate::session::{ConnectionState, Session};
use crate::widgets::preferences_dialog::background_reason;
use crate::widgets::{PreferencesDialog, Window};
use crate::{background, notifications};

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub server_manager: OnceCell<ServerManager>,
        pub session: OnceCell<Session>,
        pub settings: OnceCell<gio::Settings>,
        /// Started with --background: don't show the window on the first activation
        pub start_hidden: std::cell::Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "ParcelsApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().add_main_option(
                "background",
                glib::Char::from(0),
                glib::OptionFlags::NONE,
                glib::OptionArg::None,
                &gettext("Start in the background, without showing the window"),
                None,
            );
        }
    }

    impl ApplicationImpl for Application {
        fn handle_local_options(
            &self,
            options: &glib::VariantDict,
        ) -> std::ops::ControlFlow<glib::ExitCode> {
            if options.contains("background") {
                self.start_hidden.set(true);
            }
            self.parent_handle_local_options(options)
        }

        fn activate(&self) {
            debug!("AdwApplication<Application>::activate");
            self.parent_activate();
            let window = self.obj().main_window();
            // Started at login: connect and notify, but stay out of the way
            if !self.start_hidden.replace(false) {
                window.present();
            }
        }

        /// Called for `parcels magnet:…`, `parcels file.torrent` and when the
        /// desktop opens a magnet link or a torrent file with Parcels. With an
        /// instance already running, the files are forwarded to it.
        fn open(&self, files: &[gio::File], _hint: &str) {
            debug!("AdwApplication<Application>::open");
            let window = self.obj().main_window();
            window.present();
            window.open_files(files.to_vec());
        }

        fn startup(&self) {
            debug!("AdwApplication<Application>::startup");
            self.parent_startup();
            let app = self.obj();

            // Set icons for shell
            gtk::Window::set_default_icon_name(*APP_ID);

            app.setup_gactions();
            app.setup_accels();
            app.setup_session();
            app.setup_background();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionMap, gio::ActionGroup;
}

impl Application {
    pub fn server_manager(&self) -> ServerManager {
        self.imp()
            .server_manager
            .get_or_init(ServerManager::default)
            .clone()
    }

    pub fn settings(&self) -> gio::Settings {
        self.imp()
            .settings
            .get_or_init(|| gio::Settings::new(*APP_ID))
            .clone()
    }

    pub fn session(&self) -> Session {
        self.imp().session.get_or_init(Session::default).clone()
    }

    /// Connects to the active server now and whenever it changes.
    fn setup_session(&self) {
        let manager = self.server_manager();
        let session = self.session();
        manager.connect_active_server_notify(glib::clone!(
            #[weak]
            session,
            move |manager| session.connect_to(manager.active_server())
        ));
        session.connect_to(manager.active_server());
        notifications::setup(self.upcast_ref(), &session, &self.settings());
    }

    /// Asks the portal to run in the background, and keeps the status that
    /// GNOME shows for background apps up to date.
    fn setup_background(&self) {
        let settings = self.settings();
        if settings.boolean("run-in-background") {
            let autostart = settings.boolean("autostart");
            glib::spawn_future_local(async move {
                match background::request(background_reason(), autostart).await {
                    Ok(permission) if !permission.run_in_background => {
                        tracing::info!("Not allowed to run in the background");
                    }
                    Ok(_) => (),
                    Err(err) => tracing::debug!("Background portal unavailable: {err}"),
                }
            });
        }

        let session = self.session();
        let update_status = |session: &Session| {
            let server = session.server().map(|s| s.name()).unwrap_or_default();
            let message = match session.state() {
                // Translators: Status shown for apps running in the background; {} is a server name
                ConnectionState::Connected if session.reachable() => {
                    gettext("Connected to “{}”").replace("{}", &server)
                }
                ConnectionState::Connected | ConnectionState::Failed => {
                    // Translators: Status shown for apps running in the background; {} is a server name
                    gettext("Can't reach “{}”").replace("{}", &server)
                }
                _ => return,
            };
            glib::spawn_future_local(background::set_status(message));
        };
        session.connect_state_notify(update_status);
        session.connect_reachable_notify(update_status);
    }

    /// Returns the main window, creating it on first use.
    fn main_window(&self) -> Window {
        self.windows()
            .into_iter()
            .find_map(|w| w.downcast::<Window>().ok())
            .unwrap_or_else(|| Window::new(self))
    }

    fn setup_gactions(&self) {
        let action_quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| {
                // Close windows first so they get a chance to save their state
                for window in app.windows() {
                    match window.downcast::<Window>() {
                        Ok(window) => window.quit(),
                        Err(window) => window.close(),
                    }
                }
                app.quit();
            })
            .build();

        let action_about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about_dialog())
            .build();

        let action_preferences = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                PreferencesDialog::default().present(Some(&app.main_window()));
            })
            .build();

        // Default action of "download finished" notifications
        let action_show_torrent = gio::ActionEntry::builder("show-torrent")
            .parameter_type(Some(glib::VariantTy::STRING))
            .activate(|app: &Self, _, parameter| {
                let window = app.main_window();
                window.present();
                let torrent = parameter
                    .and_then(|p| p.get::<String>())
                    .and_then(|hash| app.session().torrents().get(&hash));
                if let Some(torrent) = torrent {
                    window.show_details(&torrent);
                }
            })
            .build();

        self.add_action_entries([
            action_quit,
            action_about,
            action_preferences,
            action_show_torrent,
        ]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("app.preferences", &["<Control>comma"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.search", &["<Control>f"]);
        self.set_accels_for_action("win.open-torrent", &["<Control>o"]);
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name(gettext("Parcels"))
            .application_icon(*APP_ID)
            .developer_name("Acauã Montiel")
            .developers(["Acauã Montiel"])
            .version(*VERSION)
            .comments(gettext(
                "Manage a qBittorrent server from your desktop. Parcels is not affiliated with the qBittorrent project.",
            ))
            .copyright("© 2026 Acauã Montiel")
            .license_type(gtk::License::Gpl30)
            .website("https://github.com/acauamontiel/parcels")
            .issue_url("https://github.com/acauamontiel/parcels/issues")
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .build();
        dialog.add_acknowledgement_section(
            Some(&gettext("Inspired By")),
            &["Fragments by Felix Häcker https://apps.gnome.org/Fragments/"],
        );
        dialog.add_credit_section(
            Some(&gettext("Based On")),
            &["GTK Rust Template by Bilal Elmoussaoui https://gitlab.gnome.org/World/Rust/gtk-rust-template"],
        );

        dialog.present(Some(&self.main_window()));
    }

    pub fn run(&self) -> glib::ExitCode {
        info!("Parcels ({})", *APP_ID);
        info!("Version: {} ({})", *VERSION, *PROFILE);
        info!("Datadir: {}", *PKGDATADIR);

        ApplicationExtManual::run(self)
    }
}

impl Default for Application {
    fn default() -> Self {
        glib::Object::builder()
            .property("application-id", *APP_ID)
            .property("resource-base-path", "/br/com/acauamontiel/Parcels/")
            .property("flags", gio::ApplicationFlags::HANDLES_OPEN)
            .build()
    }
}
