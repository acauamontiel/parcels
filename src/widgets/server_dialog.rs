use std::cell::{OnceCell, RefCell};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::glib::clone;
use qbittorrent_client::normalize_url;

use crate::model::{AuthMethod, Server, ServerManager};
use crate::{secrets, session};

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/server-dialog.ui")]
    pub struct ServerDialog {
        #[template_child]
        pub save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub url_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub name_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub invalid_certs_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub auth_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub auth_toggle: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub username_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub password_row: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub api_key_row: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub test_row: TemplateChild<adw::ButtonRow>,
        #[template_child]
        pub result_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub result_icon: TemplateChild<gtk::Stack>,
        #[template_child]
        pub remove_group: TemplateChild<adw::PreferencesGroup>,

        pub manager: OnceCell<ServerManager>,
        /// The server being edited; `None` when adding a new one
        pub server: RefCell<Option<Server>>,
        /// A scratch server holding the values typed in the dialog
        pub draft: OnceCell<Server>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ServerDialog {
        const NAME: &'static str = "ParcelsServerDialog";
        type Type = super::ServerDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_instance_callbacks();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ServerDialog {}
    impl WidgetImpl for ServerDialog {}
    impl AdwDialogImpl for ServerDialog {}
}

glib::wrapper! {
    /// Adds a server, or edits an existing one when given a server.
    pub struct ServerDialog(ObjectSubclass<imp::ServerDialog>)
        @extends gtk::Widget, adw::Dialog,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

#[gtk::template_callbacks]
impl ServerDialog {
    pub fn new(manager: &ServerManager, server: Option<&Server>) -> Self {
        let dialog: Self = glib::Object::new();
        let imp = dialog.imp();
        imp.manager.set(manager.clone()).expect("set once");
        imp.server.replace(server.cloned());
        imp.url_row.set_tooltip_text(Some(
            // Translators: {} is a list of example addresses
            &gettext("For example: {}").replace(
                "{}",
                "https://torrent.example.com, http://192.168.0.10:8080, https://example.com/qbittorrent",
            ),
        ));
        imp.draft
            .set(Server::new(server.map(Server::id).as_deref()))
            .expect("set once");

        match server {
            Some(server) => {
                dialog.set_title(&gettext("Edit Server"));
                imp.save_button.set_label(&gettext("_Save"));
                imp.remove_group.set_visible(true);
                dialog.fill(server);
            }
            None => {
                dialog.set_title(&gettext("Add Server"));
                imp.save_button.set_label(&gettext("_Add"));
                imp.auth_toggle
                    .set_active_name(Some(AuthMethod::Password.id()));
            }
        }
        dialog.on_auth_changed();

        dialog
    }

    fn fill(&self, server: &Server) {
        let imp = self.imp();
        imp.url_row.set_text(&server.url());
        imp.name_row.set_text(&server.name());
        imp.invalid_certs_row
            .set_active(server.accept_invalid_certs());
        imp.auth_toggle
            .set_active_name(Some(server.auth_method().id()));
        imp.username_row.set_text(&server.username());

        if !server.needs_secret() {
            return;
        }

        let id = server.id();
        let auth_method = server.auth_method();
        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                match secrets::lookup(&id).await {
                    Ok(Some(secret)) => dialog.secret_row(auth_method).set_text(&secret),
                    Ok(None) => (),
                    Err(err) => {
                        tracing::error!("Failed to read the keyring: {err}");
                        dialog.show_result(
                            false,
                            &gettext("Could not read the password from the keyring"),
                        );
                    }
                }
            }
        ));
    }

    fn auth_method(&self) -> AuthMethod {
        AuthMethod::from_id(&self.imp().auth_toggle.active_name().unwrap_or_default())
    }

    fn secret_row(&self, auth_method: AuthMethod) -> adw::PasswordEntryRow {
        match auth_method {
            AuthMethod::Password => self.imp().password_row.get(),
            AuthMethod::ApiKey => self.imp().api_key_row.get(),
        }
    }

    fn secret(&self) -> String {
        self.secret_row(self.auth_method()).text().to_string()
    }

    /// Copies the form into the draft server and returns it.
    fn draft(&self) -> Server {
        let imp = self.imp();
        let draft = imp.draft.get().expect("set in new()").clone();
        let url = imp.url_row.text().trim().to_owned();
        let name = match imp.name_row.text().trim() {
            "" => normalize_url(&url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_owned))
                .unwrap_or_else(|| url.clone()),
            name => name.to_owned(),
        };

        draft.set_url(url);
        draft.set_name(name);
        draft.set_accept_invalid_certs(imp.invalid_certs_row.is_active());
        draft.set_auth_method(self.auth_method());
        draft.set_username(match self.auth_method() {
            AuthMethod::Password => imp.username_row.text().trim().to_owned(),
            AuthMethod::ApiKey => String::new(),
        });
        draft
    }

    fn is_valid(&self) -> bool {
        let imp = self.imp();
        let url_valid = normalize_url(&imp.url_row.text()).is_ok();
        let credentials_valid = match self.auth_method() {
            AuthMethod::Password => {
                // No username means no authentication, but a password alone is a mistake
                !imp.username_row.text().trim().is_empty() || imp.password_row.text().is_empty()
            }
            AuthMethod::ApiKey => !imp.api_key_row.text().trim().is_empty(),
        };
        url_valid && credentials_valid
    }

    fn show_result(&self, success: bool, message: &str) {
        let imp = self.imp();
        imp.result_icon
            .set_visible_child_name(if success { "success" } else { "error" });
        imp.result_row.set_title(message);
        imp.result_row.set_visible(true);
        // Let screen readers announce the outcome
        self.announce(message, gtk::AccessibleAnnouncementPriority::Medium);
    }

    #[template_callback]
    fn on_input_changed(&self) {
        let imp = self.imp();
        let valid = self.is_valid();
        imp.save_button.set_sensitive(valid);
        imp.test_row.set_sensitive(valid);

        let url = imp.url_row.text();
        if url.is_empty() || normalize_url(&url).is_ok() {
            imp.url_row.remove_css_class("error");
        } else {
            imp.url_row.add_css_class("error");
        }
    }

    #[template_callback]
    fn on_auth_changed(&self) {
        let imp = self.imp();
        let api_key = self.auth_method() == AuthMethod::ApiKey;
        imp.username_row.set_visible(!api_key);
        imp.password_row.set_visible(!api_key);
        imp.api_key_row.set_visible(api_key);
        imp.auth_group.set_description(
            api_key
                .then(|| {
                    gettext(
                        "Requires qBittorrent 5.2 or later. Generate a key in the WebUI options.",
                    )
                })
                .as_deref(),
        );
        self.on_input_changed();
    }

    #[template_callback]
    fn on_cancel_clicked(&self) {
        self.close();
    }

    #[template_callback]
    fn on_test_activated(&self) {
        let imp = self.imp();
        let draft = self.draft();
        let config = match draft.client_config(Some(self.secret())) {
            Ok(config) => config,
            Err(err) => {
                self.show_result(false, &session::error_message(&err, draft.auth_method()));
                return;
            }
        };

        imp.test_row.set_sensitive(false);
        imp.result_icon.set_visible_child_name("testing");
        imp.result_row.set_title(&gettext("Connecting…"));
        imp.result_row.set_visible(true);

        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let result = session::connect_with_config(config).await;
                dialog.imp().test_row.set_sensitive(dialog.is_valid());
                match result {
                    Ok((_, info)) => {
                        // Translators: {} is a version number, e.g. 5.1.2
                        let message = gettext("Connected to qBittorrent {}")
                            .replace("{}", &info.version.to_string());
                        dialog.show_result(true, &message);
                    }
                    Err(err) => dialog
                        .show_result(false, &session::error_message(&err, dialog.auth_method())),
                }
            }
        ));
    }

    #[template_callback]
    fn on_save_clicked(&self) {
        if !self.is_valid() {
            return;
        }

        let imp = self.imp();
        let draft = self.draft();
        let secret = self.secret();
        imp.save_button.set_sensitive(false);

        glib::spawn_future_local(clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let result = if secret.is_empty() {
                    secrets::delete(&draft.id()).await
                } else {
                    secrets::store(&draft.id(), &draft.name(), &secret).await
                };

                if let Err(err) = result {
                    tracing::error!("Failed to write the keyring: {err}");
                    dialog.show_result(
                        false,
                        &gettext("Could not save the password in the keyring"),
                    );
                    dialog.imp().save_button.set_sensitive(true);
                    return;
                }

                dialog.commit(&draft);
                dialog.close();
            }
        ));
    }

    /// Saves the draft into the manager and makes it the active server.
    fn commit(&self, draft: &Server) {
        let imp = self.imp();
        let manager = imp.manager.get().expect("set in new()");
        let server = imp.server.borrow().clone();

        let server = match server {
            Some(server) => {
                server.set_url(draft.url());
                server.set_name(draft.name());
                server.set_accept_invalid_certs(draft.accept_invalid_certs());
                server.set_auth_method(draft.auth_method());
                server.set_username(draft.username());
                manager.update(&server);
                server
            }
            None => {
                manager.add(draft);
                draft.clone()
            }
        };

        // Also reconnects when the edited server was already active
        manager.set_active_server(Some(&server));
    }

    #[template_callback]
    fn on_remove_activated(&self) {
        let Some(server) = self.imp().server.borrow().clone() else {
            return;
        };

        let alert = adw::AlertDialog::builder()
            // Translators: {} is the server name
            .heading(gettext("Remove “{}”?").replace("{}", &server.name()))
            .body(gettext(
                "Parcels will forget this server and its password. Torrents on the server are not affected.",
            ))
            .default_response("cancel")
            .close_response("cancel")
            .build();
        alert.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("remove", &gettext("_Remove")),
        ]);
        alert.set_response_appearance("remove", adw::ResponseAppearance::Destructive);

        alert.connect_response(
            Some("remove"),
            clone!(
                #[weak(rename_to = dialog)]
                self,
                move |_, _| {
                    let manager = dialog.imp().manager.get().expect("set in new()").clone();
                    glib::spawn_future_local(clone!(
                        #[strong]
                        server,
                        async move { manager.remove(&server).await }
                    ));
                    dialog.close();
                }
            ),
        );
        alert.present(Some(self));
    }
}
