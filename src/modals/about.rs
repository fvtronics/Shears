use adw::prelude::AdwDialogExt;
use gettextrs::gettext;
use gtk::prelude::GtkApplicationExt;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, adw, gtk};

use crate::config::{APP_ID, VERSION};

pub struct AboutDialog {}

impl SimpleComponent for AboutDialog {
    type Init = ();
    type Widgets = adw::AboutDialog;
    type Input = ();
    type Output = ();
    type Root = adw::AboutDialog;

    fn init_root() -> Self::Root {
        adw::AboutDialog::builder()
            .application_icon(APP_ID)
            .license_type(gtk::License::Gpl30)
            .website("https://www.fvtronics.com/en/project/shears")
            .issue_url("https://codeberg.org/FVtronics/shears/issues")
            .application_name("Shears")
            .version(VERSION)
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2026 Francisco Vásquez Cuevas")
            .developers(vec!["Francisco Vásquez Cuevas"])
            .designers(vec!["Francisco Vásquez Cuevas"])
            .artists(vec!["Matthew Thu https://matthew-thu.netlify.app/"])
            .build()
    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {};

        let widgets = root.clone();
        widgets.present(Some(&relm4::main_application().windows()[0]));

        ComponentParts { model, widgets }
    }

    fn update_view(&self, _dialog: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
