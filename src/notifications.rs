//! Desktop notifications: finished downloads and connection problems.

use std::cell::RefCell;
use std::rc::Rc;

use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gio, glib};

use crate::session::{ConnectionState, Session};

const CONNECTION_ID: &str = "connection";

#[derive(Default)]
struct ConnectionWatch {
    /// Pending "connection lost" notification
    timer: Option<glib::SourceId>,
    /// A "connection lost" notification was sent
    notified: bool,
}

/// Sends notifications for `session` on behalf of `app`.
pub fn setup(app: &gtk::Application, session: &Session, settings: &gio::Settings) {
    session.torrents().connect_torrent_completed(glib::clone!(
        #[weak]
        app,
        #[strong]
        settings,
        move |_, torrent| {
            // No need to interrupt someone who is looking at the window
            let window_active = app.active_window().is_some_and(|w| w.is_active());
            if !settings.boolean("notify-completed") || window_active {
                return;
            }
            app.send_notification(
                Some(&format!("completed-{}", torrent.hash())),
                &completed_notification(torrent),
            );
        }
    ));

    let watch = Rc::new(RefCell::new(ConnectionWatch::default()));
    session.connect_reachable_notify(glib::clone!(
        #[weak]
        app,
        #[strong]
        settings,
        #[strong]
        watch,
        move |session| on_reachable_changed(&app, session, &settings, &watch)
    ));
    session.connect_server_notify(glib::clone!(
        #[weak]
        app,
        #[strong]
        watch,
        move |_| {
            // A new server: problems of the previous one no longer matter
            let mut watch = watch.borrow_mut();
            if let Some(timer) = watch.timer.take() {
                timer.remove();
            }
            if watch.notified {
                app.withdraw_notification(CONNECTION_ID);
                watch.notified = false;
            }
        }
    ));
}

// Notifications are built outside of the `clone!` closures above: xgettext
// misses strings inside them (check-i18n catches that).

fn completed_notification(torrent: &crate::model::Torrent) -> gio::Notification {
    let notification = gio::Notification::new(&gettext("Download Finished"));
    notification.set_body(Some(&torrent.name()));
    // Clicking it opens the torrent
    notification.set_default_action_and_target_value(
        "app.show-torrent",
        Some(&torrent.hash().to_variant()),
    );
    notification
}

fn connection_lost_notification(server_name: &str) -> gio::Notification {
    let notification = gio::Notification::new(&gettext("Connection Lost"));
    // Translators: {} is a server name
    notification.set_body(Some(
        &gettext("Parcels can't reach “{}”. It will keep trying.").replace("{}", server_name),
    ));
    notification
}

fn on_reachable_changed(
    app: &gtk::Application,
    session: &Session,
    settings: &gio::Settings,
    watch: &Rc<RefCell<ConnectionWatch>>,
) {
    let server_name = session.server().map(|s| s.name()).unwrap_or_default();

    if session.reachable() {
        let mut watch = watch.borrow_mut();
        if let Some(timer) = watch.timer.take() {
            timer.remove();
        }
        if std::mem::take(&mut watch.notified) {
            let notification = gio::Notification::new(&gettext("Connection Restored"));
            // Translators: {} is a server name
            notification.set_body(Some(
                &gettext("Parcels is connected to “{}” again").replace("{}", &server_name),
            ));
            app.send_notification(Some(CONNECTION_ID), &notification);
        }
        return;
    }

    if !settings.boolean("notify-connection") {
        return;
    }
    // Short hiccups are normal: notify only if the server stays unreachable
    let delay = settings.uint("connection-notify-delay");
    let timer = glib::timeout_add_seconds_local_once(
        delay,
        glib::clone!(
            #[weak]
            app,
            #[weak]
            session,
            #[strong]
            watch,
            move || {
                let mut watch = watch.borrow_mut();
                watch.timer = None;
                if session.reachable() || session.state() != ConnectionState::Connected {
                    return;
                }
                app.send_notification(
                    Some(CONNECTION_ID),
                    &connection_lost_notification(&server_name),
                );
                watch.notified = true;
            }
        ),
    );
    if let Some(old) = watch.borrow_mut().timer.replace(timer) {
        old.remove();
    }
}
