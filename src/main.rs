mod application;
mod config;
mod model;
mod runtime;
mod secrets;
mod session;
mod widgets;

use gettextrs::{LocaleCategory, gettext};
use gtk::{gio, glib};

use self::application::Application;
use self::config::{GETTEXT_PACKAGE, LOCALEDIR, RESOURCES_FILE};

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Prepare i18n
    gettextrs::setlocale(LocaleCategory::LcAll, "");
    gettextrs::bindtextdomain(*GETTEXT_PACKAGE, *LOCALEDIR)
        .expect("Unable to bind the text domain");
    gettextrs::textdomain(*GETTEXT_PACKAGE).expect("Unable to switch to the text domain");

    glib::set_application_name(&gettext("Parcels"));

    let res = gio::Resource::load(*RESOURCES_FILE).expect("Could not load gresource file");
    gio::resources_register(&res);

    Application::default().run()
}
