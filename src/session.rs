//! Connection to the active server.
//!
//! The session lives in the application, not in the window, so it keeps
//! running when the window is closed and Parcels stays in the background.

use std::cell::{Cell, OnceCell, RefCell};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gettextrs::gettext;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use qbittorrent_client::{Client, ClientConfig, Error, ServerInfo};

use crate::config::APP_ID;
use crate::model::{AuthMethod, Server, TorrentList};
use crate::{runtime, secrets};

/// Refresh interval while no window is visible.
const BACKGROUND_INTERVAL: Duration = Duration::from_secs(10);
/// Longest wait between retries after network errors.
const MAX_BACKOFF: Duration = Duration::from_secs(60);

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
        /// The server could be reached on the last refresh. When false the
        /// session keeps retrying with exponential backoff.
        #[property(get)]
        pub reachable: Cell<bool>,
        /// A window is visible, so refresh often
        #[property(get, set)]
        pub foreground: Cell<bool>,
        pub torrents: OnceCell<TorrentList>,
        pub settings: OnceCell<gio::Settings>,
        pub refresh_requested: Cell<bool>,
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
    impl ObjectImpl for Session {
        fn constructed(&self) {
            self.parent_constructed();
            self.reachable.set(true);
            self.torrents.get_or_init(TorrentList::default);
            self.settings.get_or_init(|| gio::Settings::new(*APP_ID));
        }
    }
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
    pub fn client(&self) -> Option<Arc<Client>> {
        self.imp().client.borrow().clone()
    }

    /// Torrents of the connected server.
    pub fn torrents(&self) -> &TorrentList {
        self.imp().torrents.get().expect("set in constructed()")
    }

    fn set_reachable(&self, reachable: bool) {
        if self.imp().reachable.replace(reachable) != reachable {
            self.notify_reachable();
        }
    }

    /// Refreshes as soon as possible, e.g. after an action or a "Retry" click.
    pub fn refresh_now(&self) {
        self.imp().refresh_requested.set(true);
    }

    fn refresh_interval(&self) -> Duration {
        if self.foreground() {
            let ms = self
                .imp()
                .settings
                .get()
                .expect("set in constructed()")
                .uint("refresh-interval");
            Duration::from_millis(u64::from(ms))
        } else {
            BACKGROUND_INTERVAL
        }
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
        self.torrents().clear();
        self.set_reachable(true);

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
                        session.imp().client.replace(Some(client.clone()));
                        session
                            .imp()
                            .server_version
                            .replace(info.version.to_string());
                        session.notify_server_version();
                        session.sync(client, attempt).await;
                    }
                    Err(message) => session.fail(&message),
                }
            }
        ));
    }

    fn fail(&self, message: &str) {
        tracing::warn!("Connection failed: {message}");
        self.imp().error_message.replace(message.to_owned());
        self.notify_error_message();
        self.set_state(ConnectionState::Failed);
    }

    /// Fetches `sync/maindata` until the connection attempt is superseded.
    ///
    /// The first response is a full snapshot; the session only becomes
    /// `Connected` after it, so the list never shows up half empty.
    async fn sync(&self, client: Arc<Client>, attempt: u64) {
        // Hold only a weak reference across awaits
        let weak = self.downgrade();
        let mut rid = 0;
        let mut failures: u32 = 0;

        loop {
            let request_client = client.clone();
            let result = runtime::run(async move { request_client.sync_maindata(rid).await }).await;

            let Some(session) = weak.upgrade() else {
                return;
            };
            if session.imp().attempt.get() != attempt {
                return;
            }

            let delay = match result {
                Ok(data) => {
                    rid = data.rid;
                    failures = 0;
                    session.torrents().apply(&data);
                    session.set_reachable(true);
                    session.set_state(ConnectionState::Connected);
                    None
                }
                Err(err) if err.is_transient() => {
                    failures += 1;
                    tracing::warn!("Refresh failed ({failures} in a row): {err}");
                    session.set_reachable(false);
                    // 2, 4, 8… seconds, up to a minute
                    Some(Duration::from_secs(1 << failures.min(6)).min(MAX_BACKOFF))
                }
                Err(err) => {
                    let auth_method = session
                        .server()
                        .map(|s| s.auth_method())
                        .unwrap_or_default();
                    session.fail(&error_message(&err, auth_method));
                    return;
                }
            };
            drop(session);

            if !wait(&weak, delay).await {
                return;
            }
        }
    }

    /// Runs an API call on the connected server, then refreshes the list
    /// so the result shows up right away. Errors are user-readable.
    pub async fn call<T, F, Fut>(&self, f: F) -> Result<T, String>
    where
        F: FnOnce(Arc<Client>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, Error>> + Send + 'static,
        T: Send + 'static,
    {
        let Some(client) = self.client() else {
            return Err(gettext("Not connected to a server"));
        };
        let result = runtime::run(async move { f(client).await }).await;
        self.refresh_now();

        result.map_err(|err| {
            tracing::warn!("Request failed: {err}");
            let auth_method = self.server().map(|s| s.auth_method()).unwrap_or_default();
            error_message(&err, auth_method)
        })
    }

    pub fn reconnect(&self) {
        self.connect_to(self.server());
    }
}

/// Waits for the refresh interval (or `fixed`, during backoff) unless a
/// refresh is requested first. Returns false when the session is gone.
async fn wait(weak: &glib::WeakRef<Session>, fixed: Option<Duration>) -> bool {
    let start = Instant::now();
    loop {
        glib::timeout_future(Duration::from_millis(250)).await;
        let Some(session) = weak.upgrade() else {
            return false;
        };
        if session.imp().refresh_requested.replace(false) {
            return true;
        }
        // Re-read the interval: the window may have been shown meanwhile
        let interval = fixed.unwrap_or_else(|| session.refresh_interval());
        if start.elapsed() >= interval {
            return true;
        }
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
