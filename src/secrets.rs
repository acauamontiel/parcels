//! Server passwords and API keys, stored with the Secret portal or Secret Service.
//!
//! Secrets are never written to GSettings or to any other file.

use std::collections::HashMap;
use std::fmt;

use gettextrs::gettext;
use tokio::sync::OnceCell;

use crate::runtime;

/// Identifies Parcels' items in the keyring; stable across profiles.
const SCHEMA: &str = "br.com.acauamontiel.Parcels.Server";

/// A keyring failure. Boxed because `oo7::Error` is large.
#[derive(Debug)]
pub struct Error(Box<oo7::Error>);

impl From<oo7::Error> for Error {
    fn from(err: oo7::Error) -> Self {
        Self(Box::new(err))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

async fn keyring() -> Result<&'static oo7::Keyring> {
    static KEYRING: OnceCell<oo7::Keyring> = OnceCell::const_new();
    Ok(KEYRING.get_or_try_init(oo7::Keyring::new).await?)
}

fn attributes(server_id: &str) -> HashMap<&'static str, String> {
    HashMap::from([
        ("xdg:schema", SCHEMA.to_owned()),
        ("server-id", server_id.to_owned()),
    ])
}

/// Stores the secret for a server, replacing any previous one.
pub async fn store(server_id: &str, server_name: &str, secret: &str) -> Result<()> {
    // Translators: Label of the keyring item; {} is the server name
    let label = gettext("Parcels server “{}”").replace("{}", server_name);
    let attributes = attributes(server_id);
    let secret = secret.to_owned();
    runtime::run(async move {
        let keyring = keyring().await?;
        keyring.unlock().await?;
        keyring
            .create_item(&label, &attributes, secret, true)
            .await?;
        Ok(())
    })
    .await
}

/// Returns the secret for a server, if one was stored.
pub async fn lookup(server_id: &str) -> Result<Option<String>> {
    let attributes = attributes(server_id);
    runtime::run(async move {
        let keyring = keyring().await?;
        keyring.unlock().await?;
        let Some(item) = keyring.search_items(&attributes).await?.into_iter().next() else {
            return Ok(None);
        };
        item.unlock().await?;
        let secret = item.secret().await?;
        Ok(Some(String::from_utf8_lossy(&secret).into_owned()))
    })
    .await
}

/// Removes the secret for a server, if any.
pub async fn delete(server_id: &str) -> Result<()> {
    let attributes = attributes(server_id);
    runtime::run(async move {
        keyring().await?.delete(&attributes).await?;
        Ok(())
    })
    .await
}
