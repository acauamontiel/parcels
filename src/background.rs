//! Running in the background and starting with the session, through the
//! Background portal. Works inside and outside the Flatpak sandbox.

use ashpd::desktop::background::{Background, BackgroundProxy, SetStatusOptions};

use crate::runtime;

/// What the portal granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permission {
    pub run_in_background: bool,
    pub autostart: bool,
}

/// Asks to run in the background, and to start with the session if
/// `autostart` is true. The desktop may ask the user the first time.
pub async fn request(reason: String, autostart: bool) -> Result<Permission, ashpd::Error> {
    runtime::run(async move {
        let response = Background::request()
            .reason(reason.as_str())
            .auto_start(autostart)
            // Started at login without showing the window
            .command(["parcels", "--background"])
            .dbus_activatable(false)
            .send()
            .await?
            .response()?;
        Ok(Permission {
            run_in_background: response.run_in_background(),
            autostart: response.auto_start(),
        })
    })
    .await
}

/// Sets the short status GNOME shows for apps running in the background.
pub async fn set_status(message: String) {
    let result = runtime::run(async move {
        let proxy = BackgroundProxy::new().await?;
        // The portal limits the message to 96 characters
        let message: String = message.chars().take(96).collect();
        proxy
            .set_status(SetStatusOptions::default().set_message(&message))
            .await
    })
    .await;
    if let Err(err) = result {
        tracing::debug!("Could not set the background status: {err}");
    }
}
