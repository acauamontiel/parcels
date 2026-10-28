use std::cell::RefCell;

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::prelude::*;

use crate::model::Torrent;
use crate::utils;

mod imp {
    use super::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/br/com/acauamontiel/Parcels/ui/torrent-row.ui")]
    pub struct TorrentRow {
        #[template_child]
        pub status_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub state_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub progress_bar: TemplateChild<gtk::ProgressBar>,
        #[template_child]
        pub details_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub details_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub speed_label: TemplateChild<gtk::Label>,
        /// The shown torrent and the handler that refreshes the row when it changes
        pub binding: RefCell<Option<(Torrent, glib::SignalHandlerId)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TorrentRow {
        const NAME: &'static str = "ParcelsTorrentRow";
        type Type = super::TorrentRow;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for TorrentRow {
        fn dispose(&self) {
            self.obj().set_torrent(None);
        }
    }

    impl WidgetImpl for TorrentRow {}
    impl BoxImpl for TorrentRow {}
}

glib::wrapper! {
    /// A row of the torrent list. Rows are recycled by the list view, so the
    /// torrent they show changes over time.
    pub struct TorrentRow(ObjectSubclass<imp::TorrentRow>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl Default for TorrentRow {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl TorrentRow {
    pub fn set_torrent(&self, torrent: Option<&Torrent>) {
        let imp = self.imp();
        if let Some((old, handler)) = imp.binding.take() {
            old.disconnect(handler);
        }

        let Some(torrent) = torrent else {
            return;
        };

        // Any property change refreshes the whole row: cheap, and simpler than
        // one binding per label
        let handler = torrent.connect_notify_local(
            None,
            glib::clone!(
                #[weak(rename_to = row)]
                self,
                move |torrent, _| row.update(torrent)
            ),
        );
        imp.binding.replace(Some((torrent.clone(), handler)));
        self.update(torrent);
    }

    fn update(&self, torrent: &Torrent) {
        let imp = self.imp();
        let state = torrent.state();

        imp.status_icon
            .set_icon_name(Some(utils::state_icon(state)));
        imp.name_label.set_label(&torrent.name());
        imp.name_label.set_tooltip_text(Some(&torrent.name()));

        let state_text = utils::state_label(state);
        imp.state_label.set_label(&state_text);
        if state.is_errored() {
            imp.state_label.add_css_class("error");
            let error = torrent.error();
            imp.state_label
                .set_tooltip_text((!error.is_empty()).then_some(error.as_str()));
        } else {
            imp.state_label.remove_css_class("error");
            imp.state_label.set_tooltip_text(None);
        }

        imp.progress_bar
            .set_fraction(torrent.progress().clamp(0.0, 1.0));
        imp.progress_bar
            .update_property(&[gtk::accessible::Property::Label(
                // Translators: Accessible label of the progress bar; {} is the torrent name
                &gettext("Progress of {}").replace("{}", &torrent.name()),
            )]);

        let details = details_text(torrent);
        let speed = speed_text(torrent);
        imp.details_box
            .set_visible(!details.is_empty() || !speed.is_empty());
        imp.details_label.set_label(&details);
        imp.speed_label.set_label(&speed);
    }
}

/// "1.2 GB of 4.5 GB (27%) · 2 h 13 min", or just the size when complete.
fn details_text(torrent: &Torrent) -> String {
    let state = torrent.state();
    let size = torrent.size();

    if !torrent.has_metadata() && size <= 0 {
        return String::new();
    }
    if state.is_completed() {
        return utils::format_size(size);
    }

    let percent = (torrent.progress() * 100.0).floor() as i64;
    // Translators: {done} and {total} are sizes, {percent} a number, e.g. "1.2 GB of 4.5 GB (27%)"
    let mut text = gettext("{done} of {total} ({percent}%)")
        .replace("{done}", &utils::format_size(size - torrent.amount_left()))
        .replace("{total}", &utils::format_size(size))
        .replace("{percent}", &percent.to_string());

    let eta = torrent.eta();
    if eta >= 0 && !state.is_stopped() && torrent.download_speed() > 0 {
        text.push_str(" · ");
        text.push_str(&utils::format_duration(eta));
    }
    text
}

/// "↓ 1.2 MB/s ↑ 300 kB/s", leaving out directions without traffic.
fn speed_text(torrent: &Torrent) -> String {
    let mut parts = Vec::new();
    if torrent.download_speed() > 0 {
        parts.push(format!(
            "↓ {}",
            utils::format_speed(torrent.download_speed())
        ));
    }
    if torrent.upload_speed() > 0 {
        parts.push(format!("↑ {}", utils::format_speed(torrent.upload_speed())));
    }
    parts.join("  ")
}
