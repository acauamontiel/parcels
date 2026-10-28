use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use gtk::glib;
use gtk::glib::prelude::*;
use gtk::subclass::prelude::*;
use qbittorrent_client::{ClientConfig, Credentials};

/// How Parcels authenticates against a server.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ParcelsAuthMethod")]
pub enum AuthMethod {
    /// Username and password; an empty username means no authentication
    #[default]
    Password,
    ApiKey,
}

impl AuthMethod {
    pub fn id(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::ApiKey => "api-key",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "api-key" => Self::ApiKey,
            _ => Self::Password,
        }
    }
}

mod imp {
    use super::*;

    // `glib::Properties` generates GObject properties (with getters, setters
    // and `notify` signals) from these fields, so widgets can bind to them.
    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::Server)]
    pub struct Server {
        /// Random, stable identifier; also the key of the secret in the keyring
        #[property(get, construct_only)]
        id: RefCell<String>,
        #[property(get, set)]
        name: RefCell<String>,
        /// Base URL of the WebUI, as typed by the user
        #[property(get, set)]
        url: RefCell<String>,
        #[property(get, set, builder(AuthMethod::default()))]
        auth_method: Cell<AuthMethod>,
        #[property(get, set)]
        username: RefCell<String>,
        #[property(get, set)]
        accept_invalid_certs: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Server {
        const NAME: &'static str = "ParcelsServer";
        type Type = super::Server;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Server {}
}

glib::wrapper! {
    /// A saved qBittorrent server. The password or API key lives in the keyring.
    pub struct Server(ObjectSubclass<imp::Server>);
}

impl Server {
    pub fn new(id: Option<&str>) -> Self {
        let id = id
            .map(str::to_owned)
            .unwrap_or_else(|| glib::uuid_string_random().to_string());
        glib::Object::builder().property("id", id).build()
    }

    /// Whether connecting needs a password or API key from the keyring.
    pub fn needs_secret(&self) -> bool {
        match self.auth_method() {
            AuthMethod::ApiKey => true,
            AuthMethod::Password => !self.username().is_empty(),
        }
    }

    /// Builds the client configuration, given the secret from the keyring.
    pub fn client_config(
        &self,
        secret: Option<String>,
    ) -> Result<ClientConfig, qbittorrent_client::Error> {
        let secret = secret.unwrap_or_default();
        let credentials = match self.auth_method() {
            AuthMethod::ApiKey => Credentials::ApiKey(secret),
            AuthMethod::Password if self.username().is_empty() => Credentials::None,
            AuthMethod::Password => Credentials::Password {
                username: self.username(),
                password: secret,
            },
        };

        let mut config = ClientConfig::new(&self.url(), credentials)?;
        config.accept_invalid_certs = self.accept_invalid_certs();
        config.user_agent = concat!("Parcels/", env!("CARGO_PKG_VERSION")).to_owned();
        Ok(config)
    }

    /// Serializes to the `a{sv}` dictionaries stored in GSettings.
    pub fn to_dict(&self) -> HashMap<String, glib::Variant> {
        HashMap::from([
            ("id".to_owned(), self.id().to_variant()),
            ("name".to_owned(), self.name().to_variant()),
            ("url".to_owned(), self.url().to_variant()),
            (
                "auth-method".to_owned(),
                self.auth_method().id().to_variant(),
            ),
            ("username".to_owned(), self.username().to_variant()),
            (
                "accept-invalid-certs".to_owned(),
                self.accept_invalid_certs().to_variant(),
            ),
        ])
    }

    /// Deserializes from GSettings; returns `None` for entries without an id or URL.
    pub fn from_dict(dict: &HashMap<String, glib::Variant>) -> Option<Self> {
        let string = |key: &str| dict.get(key).and_then(|v| v.get::<String>());
        let id = string("id").filter(|id| !id.is_empty())?;
        let url = string("url").filter(|url| !url.is_empty())?;

        let server = Self::new(Some(&id));
        server.set_url(url);
        server.set_name(string("name").unwrap_or_default());
        server.set_auth_method(AuthMethod::from_id(
            &string("auth-method").unwrap_or_default(),
        ));
        server.set_username(string("username").unwrap_or_default());
        server.set_accept_invalid_certs(
            dict.get("accept-invalid-certs")
                .and_then(|v| v.get::<bool>())
                .unwrap_or(false),
        );
        Some(server)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_dict() {
        let server = Server::new(None);
        server.set_name("thoth");
        server.set_url("https://torrent.example.com/qbt");
        server.set_auth_method(AuthMethod::ApiKey);
        server.set_username("admin");
        server.set_accept_invalid_certs(true);

        let copy = Server::from_dict(&server.to_dict()).unwrap();
        assert_eq!(copy.id(), server.id());
        assert_eq!(copy.name(), "thoth");
        assert_eq!(copy.url(), "https://torrent.example.com/qbt");
        assert_eq!(copy.auth_method(), AuthMethod::ApiKey);
        assert_eq!(copy.username(), "admin");
        assert!(copy.accept_invalid_certs());
    }

    #[test]
    fn rejects_incomplete_entries() {
        let mut dict = Server::new(None).to_dict();
        dict.insert("url".into(), "".to_variant());
        assert!(Server::from_dict(&dict).is_none());
    }

    #[test]
    fn empty_username_means_no_authentication() {
        let server = Server::new(None);
        server.set_url("http://nas:8080");
        let config = server.client_config(Some("ignored".into())).unwrap();
        assert_eq!(config.credentials, Credentials::None);
    }
}
