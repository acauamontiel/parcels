use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib::clone;
use gtk::{gio, glib};

use crate::background;
use crate::config::APP_ID;

mod imp {
    use super::*;

    #[derive(Debug, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[template_child]
        pub background_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub autostart_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub notify_completed_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub notify_connection_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub connection_delay_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub refresh_row: TemplateChild<adw::SpinRow>,
        pub settings: gio::Settings,
    }

    impl Default for PreferencesDialog {
        fn default() -> Self {
            Self {
                background_row: Default::default(),
                autostart_row: Default::default(),
                notify_completed_row: Default::default(),
                notify_connection_row: Default::default(),
                connection_delay_row: Default::default(),
                refresh_row: Default::default(),
                settings: gio::Settings::new(*APP_ID),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "ParcelsPreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesDialog {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().bind_settings();
        }
    }

    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends gtk::Widget, adw::Dialog, adw::PreferencesDialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for PreferencesDialog {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PreferencesDialog {
    fn bind_settings(&self) {
        let imp = self.imp();
        let settings = &imp.settings;

        settings
            .bind("run-in-background", &*imp.background_row, "active")
            .build();
        settings
            .bind("autostart", &*imp.autostart_row, "active")
            .build();
        settings
            .bind("run-in-background", &*imp.autostart_row, "sensitive")
            .get_only()
            .build();
        settings
            .bind("notify-completed", &*imp.notify_completed_row, "active")
            .build();
        settings
            .bind("notify-connection", &*imp.notify_connection_row, "active")
            .build();
        settings
            .bind("notify-connection", &*imp.connection_delay_row, "sensitive")
            .get_only()
            .build();
        settings
            .bind(
                "connection-notify-delay",
                &*imp.connection_delay_row,
                "value",
            )
            .mapping(|variant, _| variant.get::<u32>().map(|v| f64::from(v).to_value()))
            .set_mapping(|value, _| {
                value
                    .get::<f64>()
                    .ok()
                    .map(|v| (v.round() as u32).to_variant())
            })
            .build();
        // Stored in milliseconds, shown in seconds
        settings
            .bind("refresh-interval", &*imp.refresh_row, "value")
            .mapping(|variant, _| {
                variant
                    .get::<u32>()
                    .map(|ms| (f64::from(ms) / 1000.0).to_value())
            })
            .set_mapping(|value, _| {
                value
                    .get::<f64>()
                    .ok()
                    .map(|seconds| ((seconds * 1000.0).round() as u32).to_variant())
            })
            .build();

        // Ask the portal whenever these change; it may refuse
        for key in ["run-in-background", "autostart"] {
            settings.connect_changed(
                Some(key),
                clone!(
                    #[weak(rename_to = dialog)]
                    self,
                    move |_, _| dialog.request_background()
                ),
            );
        }
    }

    fn request_background(&self) {
        let settings = self.imp().settings.clone();
        if !settings.boolean("run-in-background") {
            // Nothing to ask; stop starting with the session too
            if settings.boolean("autostart") {
                let _ = settings.set_boolean("autostart", false);
            }
            return;
        }
        let autostart = settings.boolean("autostart");
        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                match background::request(background_reason(), autostart).await {
                    Ok(permission) => {
                        if !permission.run_in_background {
                            let _ = settings.set_boolean("run-in-background", false);
                            dialog.add_toast(adw::Toast::new(&gettext(
                                "Parcels is not allowed to run in the background. Check the app's permissions in Settings.",
                            )));
                        } else if autostart && !permission.autostart {
                            let _ = settings.set_boolean("autostart", false);
                            dialog.add_toast(adw::Toast::new(&gettext(
                                "Parcels is not allowed to start automatically",
                            )));
                        }
                    }
                    Err(err) => {
                        tracing::warn!("Background portal request failed: {err}");
                        dialog.add_toast(adw::Toast::new(&gettext(
                            "Could not ask the system to run Parcels in the background",
                        )));
                    }
                }
            }
        ));
    }
}

/// Shown by the desktop when it asks the user.
pub fn background_reason() -> String {
    gettext("Parcels keeps checking your qBittorrent server to notify you when downloads finish")
}
