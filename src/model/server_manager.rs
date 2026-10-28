use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;

use gtk::gio;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

use super::Server;
use crate::config::APP_ID;
use crate::secrets;

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::ServerManager)]
    pub struct ServerManager {
        pub servers: OnceCell<gio::ListStore>,
        pub settings: OnceCell<gio::Settings>,
        /// The server the app is connected (or connecting) to
        #[property(get, nullable)]
        pub active_server: RefCell<Option<Server>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ServerManager {
        const NAME: &'static str = "ParcelsServerManager";
        type Type = super::ServerManager;
    }

    #[glib::derived_properties]
    impl ObjectImpl for ServerManager {
        fn constructed(&self) {
            self.parent_constructed();
            self.servers.get_or_init(gio::ListStore::new::<Server>);
            self.settings.get_or_init(|| gio::Settings::new(*APP_ID));
            self.obj().load();
        }
    }
}

glib::wrapper! {
    /// The saved servers, persisted in GSettings (without secrets).
    pub struct ServerManager(ObjectSubclass<imp::ServerManager>);
}

impl Default for ServerManager {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ServerManager {
    /// List of [`Server`] objects, for menus and list views.
    pub fn servers(&self) -> &gio::ListStore {
        self.imp().servers.get().expect("set in constructed()")
    }

    fn settings(&self) -> &gio::Settings {
        self.imp().settings.get().expect("set in constructed()")
    }

    fn iter(&self) -> impl Iterator<Item = Server> + '_ {
        self.servers().iter::<Server>().filter_map(Result::ok)
    }

    pub fn find(&self, id: &str) -> Option<Server> {
        self.iter().find(|server| server.id() == id)
    }

    fn position(&self, server: &Server) -> Option<u32> {
        self.servers().find(server)
    }

    fn load(&self) {
        let dicts = self
            .settings()
            .value("servers")
            .get::<Vec<HashMap<String, glib::Variant>>>()
            .unwrap_or_default();
        let servers: Vec<Server> = dicts.iter().filter_map(Server::from_dict).collect();
        self.servers().splice(0, 0, &servers);

        let active_id = self.settings().string("active-server");
        let active = self.find(&active_id).or_else(|| servers.first().cloned());
        self.imp().active_server.replace(active);
    }

    fn save(&self) {
        let dicts: Vec<HashMap<String, glib::Variant>> =
            self.iter().map(|server| server.to_dict()).collect();
        if let Err(err) = self.settings().set_value("servers", &dicts.to_variant()) {
            tracing::error!("Failed to save servers: {err}");
        }
    }

    /// Makes `server` the active one. Also re-announces an unchanged server,
    /// which makes the session reconnect.
    pub fn set_active_server(&self, server: Option<&Server>) {
        self.imp().active_server.replace(server.cloned());
        let id = server.map(Server::id).unwrap_or_default();
        if let Err(err) = self.settings().set_string("active-server", &id) {
            tracing::error!("Failed to save the active server: {err}");
        }
        self.notify_active_server();
    }

    pub fn add(&self, server: &Server) {
        self.servers().append(server);
        self.save();
    }

    /// Persists changes made to `server`'s properties.
    pub fn update(&self, server: &Server) {
        if let Some(position) = self.position(server) {
            // Lets menus pick up a new name
            self.servers().items_changed(position, 1, 1);
        }
        self.save();
    }

    /// Forgets `server` and deletes its secret.
    pub async fn remove(&self, server: &Server) {
        if let Some(position) = self.position(server) {
            self.servers().remove(position);
        }
        self.save();

        if self.active_server().as_ref() == Some(server) {
            let next = self.iter().next();
            self.set_active_server(next.as_ref());
        }

        if let Err(err) = secrets::delete(&server.id()).await {
            tracing::warn!("Failed to delete the secret of a removed server: {err}");
        }
    }
}
