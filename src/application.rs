use gettextrs::gettext;
use tracing::{debug, info};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::config::{APP_ID, PKGDATADIR, PROFILE, VERSION};
use crate::model::ServerManager;
use crate::session::Session;
use crate::widgets::Window;

mod imp {
    use std::cell::OnceCell;

    use super::*;

    #[derive(Debug, Default)]
    pub struct Application {
        pub server_manager: OnceCell<ServerManager>,
        pub session: OnceCell<Session>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "ParcelsApplication";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        fn activate(&self) {
            debug!("AdwApplication<Application>::activate");
            self.parent_activate();
            self.obj().main_window().present();
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
                    window.close();
                }
                app.quit();
            })
            .build();

        let action_about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about_dialog())
            .build();

        self.add_action_entries([action_quit, action_about]);
    }

    fn setup_accels(&self) {
        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.search", &["<Control>f"]);
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutDialog::builder()
            .application_name(gettext("Parcels"))
            .application_icon(*APP_ID)
            .developer_name("Acauã Montiel")
            .developers(["Acauã Montiel"])
            .version(*VERSION)
            .license_type(gtk::License::Gpl30)
            .website("https://github.com/acauamontiel/parcels")
            .issue_url("https://github.com/acauamontiel/parcels/issues")
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .build();

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
            .build()
    }
}
