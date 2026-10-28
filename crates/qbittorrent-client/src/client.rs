use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::header::REFERER;
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use url::Url;

use crate::{
    AddTorrents, Error, FilePriority, MIN_SERVER_VERSION, MainData, PeersData, Result, ServerInfo,
    TorrentFile, TorrentProperties, Tracker, Version,
};

/// How to authenticate against the WebUI.
#[derive(Clone, PartialEq, Eq)]
pub enum Credentials {
    /// The server bypasses authentication for this client (e.g. whitelisted subnet).
    None,
    /// Cookie-based session: `auth/login` returns an `SID` cookie.
    Password { username: String, password: String },
    /// Stateless `Authorization: Bearer` header (qBittorrent 5.2+).
    ApiKey(String),
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never print secrets, not even in debug logs
        match self {
            Self::None => f.write_str("None"),
            Self::Password { username, .. } => f
                .debug_struct("Password")
                .field("username", username)
                .finish_non_exhaustive(),
            Self::ApiKey(_) => f.write_str("ApiKey(..)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the WebUI, always ending with `/`.
    pub url: Url,
    pub credentials: Credentials,
    /// Accept self-signed or otherwise invalid TLS certificates.
    pub accept_invalid_certs: bool,
    pub timeout: Duration,
    pub user_agent: String,
}

impl ClientConfig {
    /// Creates a configuration from an address typed by the user.
    ///
    /// The address may omit the scheme (HTTPS is assumed) and may include a
    /// port and a path prefix, for instances behind a reverse proxy.
    pub fn new(address: &str, credentials: Credentials) -> Result<Self> {
        Ok(Self {
            url: normalize_url(address)?,
            credentials,
            accept_invalid_certs: false,
            timeout: Duration::from_secs(30),
            user_agent: concat!("qbittorrent-client-rs/", env!("CARGO_PKG_VERSION")).to_owned(),
        })
    }
}

/// Normalizes a user-typed server address into a base URL ending with `/`.
pub fn normalize_url(address: &str) -> Result<Url> {
    let address = address.trim();
    if address.is_empty() {
        return Err(Error::InvalidUrl(address.to_owned()));
    }

    let with_scheme = if address.contains("://") {
        address.to_owned()
    } else {
        format!("https://{address}")
    };

    let mut url = Url::parse(&with_scheme).map_err(|_| Error::InvalidUrl(address.to_owned()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(Error::InvalidUrl(address.to_owned()));
    }

    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }

    Ok(url)
}

/// A connection to one qBittorrent server.
///
/// Cheap to share behind an `Arc`; all methods take `&self`. With password
/// credentials the session cookie is renewed transparently when the server
/// answers 403.
pub struct Client {
    http: reqwest::Client,
    config: ClientConfig,
    /// Incremented on every successful login. Lets concurrent requests that
    /// hit an expired session share a single re-login.
    login_generation: AtomicU64,
    login_lock: tokio::sync::Mutex<()>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("url", &self.config.url.as_str())
            .field("credentials", &self.config.credentials)
            .finish_non_exhaustive()
    }
}

impl Client {
    pub fn new(config: ClientConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(config.timeout)
            .connect_timeout(Duration::from_secs(10))
            .tls_danger_accept_invalid_certs(config.accept_invalid_certs)
            .user_agent(&config.user_agent)
            .build()
            .map_err(Error::from_reqwest)?;

        Ok(Self {
            http,
            config,
            login_generation: AtomicU64::new(0),
            login_lock: tokio::sync::Mutex::new(()),
        })
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Authenticates if needed, then checks the server is a supported qBittorrent.
    pub async fn connect(&self) -> Result<ServerInfo> {
        if matches!(self.config.credentials, Credentials::Password { .. }) {
            self.login().await?;
        }

        let version = self.get_version("app/version").await?;
        let api_version = self.get_version("app/webapiVersion").await?;

        if version < MIN_SERVER_VERSION {
            return Err(Error::UnsupportedVersion {
                found: version,
                minimum: MIN_SERVER_VERSION,
            });
        }

        Ok(ServerInfo {
            version,
            api_version,
        })
    }

    /// Logs in with the configured username and password.
    ///
    /// Does nothing for other kinds of credentials.
    pub async fn login(&self) -> Result<()> {
        let _guard = self.login_lock.lock().await;
        self.login_locked().await
    }

    /// Logs in again unless another request already did since `generation`.
    async fn relogin(&self, generation: u64) -> Result<()> {
        let _guard = self.login_lock.lock().await;
        if self.login_generation.load(Ordering::SeqCst) != generation {
            return Ok(());
        }
        tracing::debug!("Session expired, logging in again");
        self.login_locked().await
    }

    /// Must be called with `login_lock` held.
    async fn login_locked(&self) -> Result<()> {
        let Credentials::Password { username, password } = &self.config.credentials else {
            return Ok(());
        };

        let response = self
            .request(Method::POST, "auth/login")?
            .form(&[("username", username), ("password", password)])
            .send()
            .await
            .map_err(Error::from_reqwest)?;

        let status = response.status();
        let body = response.text().await.map_err(Error::from_reqwest)?;
        let body = body.trim();
        match status {
            // Before WebAPI 2.14.0 a failed login is a 200 with "Fails."
            StatusCode::OK if body == "Fails." => Err(Error::InvalidCredentials),
            // A reverse proxy or another web app answering instead of qBittorrent
            status if status.is_success() && body.starts_with('<') => Err(Error::NotQbittorrent),
            // 200 "Ok." before qBittorrent 5.3, 204 without a body since
            status if status.is_success() => {
                self.login_generation.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            StatusCode::NOT_FOUND => Err(Error::NotQbittorrent),
            StatusCode::UNAUTHORIZED => Err(Error::InvalidCredentials),
            StatusCode::FORBIDDEN => Err(Error::Banned),
            status => Err(Error::Http {
                status,
                body: body.to_owned(),
            }),
        }
    }

    /// Fetches the changes since response `rid` (0 for a full snapshot).
    pub async fn sync_maindata(&self, rid: i64) -> Result<MainData> {
        self.get_json("sync/maindata", &[("rid", rid.to_string())])
            .await
    }

    /// Adds torrents from URLs (magnet or HTTP) and `.torrent` files.
    ///
    /// Succeeds if at least one torrent was added or is still being fetched.
    pub async fn add(&self, torrents: &AddTorrents) -> Result<()> {
        let response = match self
            .execute(Method::POST, "torrents/add", |r| {
                r.multipart(torrents.to_form())
            })
            .await
        {
            // 409: none added (WebAPI 2.14+); 415: invalid torrent file
            Err(Error::Http { status, .. }) if matches!(status.as_u16(), 409 | 415) => {
                return Err(Error::TorrentRejected);
            }
            result => result?,
        };

        let body = response.text().await.map_err(Error::from_reqwest)?;
        // Before WebAPI 2.14 the body is "Ok." or "Fails."; since then, counts in JSON
        if body.trim() == "Fails." {
            return Err(Error::TorrentRejected);
        }
        if let Ok(result) = serde_json::from_str::<AddResult>(&body)
            && result.success_count == 0
            && result.pending_count == 0
            && result.failure_count > 0
        {
            return Err(Error::TorrentRejected);
        }
        Ok(())
    }

    /// The server's default download folder.
    pub async fn default_save_path(&self) -> Result<String> {
        self.get_text("app/defaultSavePath").await
    }

    /// General information about one torrent.
    pub async fn properties(&self, hash: &str) -> Result<TorrentProperties> {
        self.get_torrent_json("torrents/properties", hash, &[])
            .await
    }

    /// The files of a torrent; empty until its metadata is known.
    pub async fn files(&self, hash: &str) -> Result<Vec<TorrentFile>> {
        self.get_torrent_json("torrents/files", hash, &[]).await
    }

    pub async fn trackers(&self, hash: &str) -> Result<Vec<Tracker>> {
        self.get_torrent_json("torrents/trackers", hash, &[]).await
    }

    /// Changes to the peers of a torrent since response `rid`.
    pub async fn torrent_peers(&self, hash: &str, rid: i64) -> Result<PeersData> {
        self.get_torrent_json("sync/torrentPeers", hash, &[("rid", rid.to_string())])
            .await
    }

    /// Sets the download priority of files, by index.
    pub async fn set_file_priority(
        &self,
        hash: &str,
        indexes: &[u32],
        priority: FilePriority,
    ) -> Result<()> {
        let ids: Vec<String> = indexes.iter().map(u32::to_string).collect();
        self.post_form(
            "torrents/filePrio",
            &[
                ("hash", hash.to_owned()),
                ("id", ids.join("|")),
                ("priority", priority.value().to_string()),
            ],
        )
        .await
    }

    /// Like `get_json`, mapping 404 to [`Error::TorrentNotFound`].
    async fn get_torrent_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        hash: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let mut params = vec![("hash", hash.to_owned())];
        params.extend(query.iter().cloned());
        match self.get_json(endpoint, &params).await {
            Err(Error::Http {
                status: StatusCode::NOT_FOUND,
                ..
            }) => Err(Error::TorrentNotFound),
            result => result,
        }
    }

    /// Starts (resumes) torrents. Was `torrents/resume` before qBittorrent 5.0.
    pub async fn start(&self, hashes: &[String]) -> Result<()> {
        self.post_form("torrents/start", &[("hashes", join_hashes(hashes))])
            .await
    }

    /// Stops (pauses) torrents. Was `torrents/pause` before qBittorrent 5.0.
    pub async fn stop(&self, hashes: &[String]) -> Result<()> {
        self.post_form("torrents/stop", &[("hashes", join_hashes(hashes))])
            .await
    }

    /// Removes torrents, and their downloaded data if `delete_files` is true.
    pub async fn delete(&self, hashes: &[String], delete_files: bool) -> Result<()> {
        self.post_form(
            "torrents/delete",
            &[
                ("hashes", join_hashes(hashes)),
                ("deleteFiles", delete_files.to_string()),
            ],
        )
        .await
    }

    /// Sets the category of torrents; an empty `category` removes it.
    pub async fn set_category(&self, hashes: &[String], category: &str) -> Result<()> {
        self.post_form(
            "torrents/setCategory",
            &[
                ("hashes", join_hashes(hashes)),
                ("category", category.to_owned()),
            ],
        )
        .await
    }

    /// Creates a category. An empty `save_path` uses the default path.
    pub async fn create_category(&self, name: &str, save_path: &str) -> Result<()> {
        self.post_form(
            "torrents/createCategory",
            &[
                ("category", name.to_owned()),
                ("savePath", save_path.to_owned()),
            ],
        )
        .await
    }

    /// Adds tags to torrents, creating tags that don't exist yet.
    pub async fn add_tags(&self, hashes: &[String], tags: &[String]) -> Result<()> {
        self.post_form(
            "torrents/addTags",
            &[("hashes", join_hashes(hashes)), ("tags", tags.join(","))],
        )
        .await
    }

    pub async fn remove_tags(&self, hashes: &[String], tags: &[String]) -> Result<()> {
        self.post_form(
            "torrents/removeTags",
            &[("hashes", join_hashes(hashes)), ("tags", tags.join(","))],
        )
        .await
    }

    /// Moves the data of torrents to `location`, a path on the server.
    pub async fn set_location(&self, hashes: &[String], location: &str) -> Result<()> {
        self.post_form(
            "torrents/setLocation",
            &[
                ("hashes", join_hashes(hashes)),
                ("location", location.to_owned()),
            ],
        )
        .await
    }

    /// Renames a torrent (not its files).
    pub async fn rename(&self, hash: &str, name: &str) -> Result<()> {
        self.post_form(
            "torrents/rename",
            &[("hash", hash.to_owned()), ("name", name.to_owned())],
        )
        .await
    }

    /// Switches between the normal and the alternative speed limits.
    pub async fn toggle_alternative_speed_limits(&self) -> Result<()> {
        self.post_form("transfer/toggleSpeedLimitsMode", &[]).await
    }

    async fn post_form(&self, endpoint: &str, form: &[(&str, String)]) -> Result<()> {
        self.execute(Method::POST, endpoint, |r| r.form(form))
            .await
            .map(drop)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &[(&str, String)],
    ) -> Result<T> {
        let bytes = self
            .execute(Method::GET, endpoint, |r| r.query(query))
            .await?
            .bytes()
            .await
            .map_err(Error::from_reqwest)?;
        serde_json::from_slice(&bytes).map_err(|err| Error::Parse(err.to_string()))
    }

    async fn get_version(&self, endpoint: &str) -> Result<Version> {
        let text = match self.get_text(endpoint).await {
            Err(Error::Http {
                status: StatusCode::NOT_FOUND,
                ..
            }) => return Err(Error::NotQbittorrent),
            result => result?,
        };
        text.parse().map_err(|_| Error::NotQbittorrent)
    }

    async fn get_text(&self, endpoint: &str) -> Result<String> {
        self.execute(Method::GET, endpoint, |r| r)
            .await?
            .text()
            .await
            .map_err(Error::from_reqwest)
    }

    /// Builds a request for `api/v2/{endpoint}` with the headers every call needs.
    fn request(&self, method: Method, endpoint: &str) -> Result<RequestBuilder> {
        let url = self
            .config
            .url
            .join(&format!("api/v2/{endpoint}"))
            .map_err(|_| Error::InvalidUrl(self.config.url.to_string()))?;

        // qBittorrent rejects cross-site requests: Referer must match the host
        let mut builder = self
            .http
            .request(method, url)
            .header(REFERER, self.config.url.as_str());
        if let Credentials::ApiKey(key) = &self.config.credentials {
            builder = builder.bearer_auth(key);
        }
        Ok(builder)
    }

    /// Sends a request, logging in again once if the session expired.
    ///
    /// `build` adds the parameters; it may run twice, so the body can be rebuilt.
    async fn execute(
        &self,
        method: Method,
        endpoint: &str,
        build: impl Fn(RequestBuilder) -> RequestBuilder,
    ) -> Result<Response> {
        let generation = self.login_generation.load(Ordering::SeqCst);
        let send = || async {
            build(self.request(method.clone(), endpoint)?)
                .send()
                .await
                .map_err(Error::from_reqwest)
        };

        let mut response = send().await?;
        if response.status() == StatusCode::FORBIDDEN
            && matches!(self.config.credentials, Credentials::Password { .. })
        {
            self.relogin(generation).await?;
            response = send().await?;
        }

        check_status(response).await
    }
}

#[derive(serde::Deserialize)]
#[serde(default)]
#[derive(Default)]
struct AddResult {
    success_count: u32,
    failure_count: u32,
    pending_count: u32,
}

/// Hashes are sent as a single parameter, separated by `|`.
fn join_hashes(hashes: &[String]) -> String {
    hashes.join("|")
}

async fn check_status(response: Response) -> Result<Response> {
    match response.status() {
        status if status.is_success() => Ok(response),
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(Error::Unauthorized),
        status => {
            let body = response.text().await.unwrap_or_default();
            Err(Error::Http { status, body })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_addresses() {
        let n = |s| normalize_url(s).map(|u| u.to_string());
        assert_eq!(
            n("torrent.example.com").unwrap(),
            "https://torrent.example.com/"
        );
        assert_eq!(n(" http://nas:8080 ").unwrap(), "http://nas:8080/");
        assert_eq!(
            n("https://example.com/qbt").unwrap(),
            "https://example.com/qbt/"
        );
        assert_eq!(
            n("https://example.com/qbt/?x=1#y").unwrap(),
            "https://example.com/qbt/"
        );
        assert_eq!(
            n("192.168.0.10:8080").unwrap(),
            "https://192.168.0.10:8080/"
        );
    }

    #[test]
    fn rejects_invalid_addresses() {
        assert!(normalize_url("").is_err());
        assert!(normalize_url("ftp://example.com").is_err());
        assert!(normalize_url("https://").is_err());
        assert!(normalize_url("http://exa mple.com").is_err());
    }

    #[test]
    fn debug_hides_secrets() {
        let creds = Credentials::Password {
            username: "admin".into(),
            password: "hunter2".into(),
        };
        assert!(!format!("{creds:?}").contains("hunter2"));
        assert!(!format!("{:?}", Credentials::ApiKey("qbt_secret".into())).contains("qbt_secret"));
    }
}
