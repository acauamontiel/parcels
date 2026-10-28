//! Optional tests against a real qBittorrent server.
//!
//! Skipped unless `QBIT_URL` is set. Credentials come from `QBIT_USER` and
//! `QBIT_PASS`, or `QBIT_API_KEY`. Never commit real credentials.

use qbittorrent_client::{Client, ClientConfig, Credentials, MIN_SERVER_VERSION};

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

fn credentials_from_env() -> Vec<Credentials> {
    let mut all = Vec::new();
    if let (Some(username), Some(password)) = (var("QBIT_USER"), var("QBIT_PASS")) {
        all.push(Credentials::Password { username, password });
    }
    if let Some(key) = var("QBIT_API_KEY") {
        all.push(Credentials::ApiKey(key));
    }
    all
}

#[tokio::test]
async fn connects_to_live_server() {
    let Some(url) = var("QBIT_URL") else {
        eprintln!("QBIT_URL not set, skipping");
        return;
    };

    for credentials in credentials_from_env() {
        let config = ClientConfig::new(&url, credentials.clone()).unwrap();
        let info = Client::new(config)
            .unwrap()
            .connect()
            .await
            .unwrap_or_else(|err| panic!("{credentials:?}: {err}"));
        eprintln!("{credentials:?}: qBittorrent {}", info.version);
        assert!(info.version >= MIN_SERVER_VERSION);
    }
}
