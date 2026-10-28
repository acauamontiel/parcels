//! Connection to the active server.
//!
//! The session lives in the application, not in the window, so it keeps
//! running when the window is closed and Parcels stays in the background.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use gettextrs::gettext;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use qbittorrent_client::{Client, ClientConfig, Error, ServerInfo};

use crate::model::{AuthMethod, Server};
use crate::{runtime, secrets};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, glib::Enum)]
#[enum_type(name = "ParcelsConnectionState")]
pub enum ConnectionState {
    /// No server configured
    #[default]
    Idle,
    Connecting,
    Connected,
    Failed,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, glib::Properties)]
    #[properties(wrapper_type = super::Session)]
    pub struct Session {
        #[property(get, builder(ConnectionState::default()))]
        pub state: Cell<ConnectionState>,
        #[property(get, nullable)]
        pub server: RefCell<Option<Server>>,
        /// Human-readable reason of the last failure
        #[property(get)]
        pub error_message: RefCell<String>,
        /// qBittorrent version of the connected server
        #[property(get)]
        pub server_version: RefCell<String>,
        pub client: RefCell<Option<Arc<Client>>>,
        /// Incremented on every connection attempt, so late results of an
        /// abandoned attempt are ignored.
        pub attempt: Cell<u64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "ParcelsSession";
        type Type = super::Session;
    }

    #[glib::derived_properties]
    impl ObjectImpl for Session {}
}

glib::wrapper! {
    pub struct Session(ObjectSubclass<imp::Session>);
}

impl Default for Session {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Session {
    /// The client of the connected server, for making API calls.
    #[allow(dead_code)] // Used from phase 2 on
    pub fn client(&self) -> Option<Arc<Client>> {
        self.imp().client.borrow().clone()
    }

    fn set_state(&self, state: ConnectionState) {
        if self.imp().state.replace(state) != state {
            self.notify_state();
        }
    }

    /// Drops the current connection and connects to `server`.
    pub fn connect_to(&self, server: Option<Server>) {
        let imp = self.imp();
        let attempt = imp.attempt.get() + 1;
        imp.attempt.set(attempt);
        imp.client.replace(None);
        imp.server.replace(server.clone());
        self.notify_server();

        let Some(server) = server else {
            self.set_state(ConnectionState::Idle);
            return;
        };

        self.set_state(ConnectionState::Connecting);

        // `spawn_future_local` runs the future on the GLib main loop, on this
        // thread, so it may touch GObjects. `clone!` hands the closure a weak
        // reference: if the session is gone when the future resumes, it stops
        // instead of keeping the session alive.
        glib::spawn_future_local(clone!(
            #[weak(rename_to = session)]
            self,
            async move {
                let result = connect(&server).await;
                if session.imp().attempt.get() != attempt {
                    return;
                }

                match result {
                    Ok((client, info)) => {
                        tracing::info!("Connected to qBittorrent {}", info.version);
                        session.imp().client.replace(Some(client));
                        session
                            .imp()
                            .server_version
                            .replace(info.version.to_string());
                        session.notify_server_version();
                        session.set_state(ConnectionState::Connected);
                    }
                    Err(message) => {
                        tracing::warn!("Connection failed: {message}");
                        session.imp().error_message.replace(message);
                        session.notify_error_message();
                        session.set_state(ConnectionState::Failed);
                    }
                }
            }
        ));
    }

    pub fn reconnect(&self) {
        self.connect_to(self.server());
    }
}

/// Reads the server's secret and connects. Errors are already user-readable.
async fn connect(server: &Server) -> Result<(Arc<Client>, ServerInfo), String> {
    let secret = if server.needs_secret() {
        secrets::lookup(&server.id()).await.map_err(|err| {
            tracing::error!("Failed to read the keyring: {err}");
            gettext("Could not read the password from the keyring")
        })?
    } else {
        None
    };
    let config = server
        .client_config(secret)
        .map_err(|err| error_message(&err, server.auth_method()))?;
    connect_with_config(config)
        .await
        .map_err(|err| error_message(&err, server.auth_method()))
}

/// Connects with `config`, doing all network I/O on the Tokio runtime.
pub async fn connect_with_config(config: ClientConfig) -> Result<(Arc<Client>, ServerInfo), Error> {
    runtime::run(async move {
        let client = Arc::new(Client::new(config)?);
        let info = client.connect().await?;
        Ok((client, info))
    })
    .await
}

/// Describes a client error for the user.
pub fn error_message(err: &Error, auth_method: AuthMethod) -> String {
    match err {
        Error::InvalidUrl(_) => gettext("The server address is not valid"),
        Error::Network(_) => {
            gettext("Could not reach the server. Check the address and your network connection.")
        }
        Error::Certificate(_) => gettext(
            "The server certificate is not trusted. If this server uses a self-signed certificate, turn on “Accept Invalid Certificates” in the server settings.",
        ),
        Error::InvalidCredentials => gettext("Invalid username or password"),
        Error::Banned => gettext(
            "This device is temporarily banned by qBittorrent after too many failed login attempts. Try again later.",
        ),
        Error::Unauthorized => match auth_method {
            AuthMethod::ApiKey => {
                gettext("The API key was rejected. API keys require qBittorrent 5.2 or later.")
            }
            AuthMethod::Password => gettext("The server requires a username and password"),
        },
        Error::NotQbittorrent => gettext("No qBittorrent WebUI was found at this address"),
        Error::UnsupportedVersion { found, minimum } => {
            // Translators: {found} and {minimum} are version numbers, e.g. 4.6.7 and 5.0.0
            gettext("qBittorrent {found} is not supported. Parcels requires version {minimum} or later.")
                .replace("{found}", &found.to_string())
                .replace("{minimum}", &minimum.to_string())
        }
        Error::Http { status, .. } => {
            // Translators: {} is an HTTP status, e.g. "502 Bad Gateway"
            gettext("The server returned an unexpected error ({})")
                .replace("{}", &status.to_string())
        }
        Error::Parse(_) => gettext("The server sent a response Parcels could not understand"),
    }
}
