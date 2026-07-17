use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::factory::DynamicIndex;
use relm4::gtk::prelude::EditableExt;
use relm4::{Component, ComponentParts, ComponentSender, adw, gtk};

pub struct PasswordDialog {
    pub file_index: Option<DynamicIndex>,
    pub filename: String,
    pub is_error: bool,
    pub is_valid: bool,
    password_entry: Option<gtk::PasswordEntry>,
}

#[derive(Clone, PartialEq, Eq, Default)]
pub struct SecretString(pub String);

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl std::ops::Deref for SecretString {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        self.0.as_str()
    }
}

impl From<String> for SecretString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SecretString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

#[derive(Debug)]
pub enum PasswordDialogMsg {
    Show {
        index: Option<DynamicIndex>,
        filename: String,
        is_error: bool,
        parent_window: gtk::Window,
    },
    PasswordChanged(SecretString),
    Unlock(SecretString),
    Cancel,
}

#[derive(Debug)]
pub enum PasswordDialogOutput {
    Unlock {
        index: Option<DynamicIndex>,
        password: SecretString,
    },
    Cancelled(Option<DynamicIndex>),
}

#[relm4::component(pub)]
impl Component for PasswordDialog {
    type Init = ();
    type Input = PasswordDialogMsg;
    type Output = PasswordDialogOutput;
    type CommandOutput = ();

    view! {
        #[root]
        dialog = adw::AlertDialog {
            set_heading: Some(&gettext("Password Required")),
            #[watch]
            set_body: &gettext("Enter the password for {filename}.").replace("{filename}", &model.filename),
            set_close_response: "cancelled",
            set_default_response: Some("unlock"),

            add_response: ("cancelled", &gettext("Cancel")),
            add_response: ("unlock", &gettext("Unlock")),

            set_response_appearance: ("unlock", adw::ResponseAppearance::Suggested),
            #[watch]
            set_response_enabled: ("unlock", model.is_valid),

            #[wrap(Some)]
            set_extra_child = &gtk::Box {
                set_halign: gtk::Align::Center,
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 12,

                #[name(password_entry)]
                gtk::PasswordEntry {
                    set_placeholder_text: Some(&gettext("Password")),
                    set_show_peek_icon: true,
                    set_activates_default: true,
                    set_width_chars: 32,
                    set_margin_bottom: 10,

                    connect_changed[sender] => move |entry| {
                        sender.input(PasswordDialogMsg::PasswordChanged(entry.text().to_string().into()));
                    },

                    connect_map => move |entry| {
                        entry.grab_focus();
                    }
                },

                gtk::Label {
                    #[watch]
                    set_visible: model.is_error,
                    set_label: &gettext("Invalid password"),
                    add_css_class: "error",
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            file_index: None,
            filename: String::new(),
            is_error: false,
            is_valid: false,
            password_entry: None,
        };
        let widgets = view_output!();

        let sender_clone = sender.clone();
        let password_entry = widgets.password_entry.clone();
        widgets.dialog.connect_response(None, move |_, response| {
            if response == "unlock" {
                sender_clone.input(PasswordDialogMsg::Unlock(
                    password_entry.text().to_string().into(),
                ));
            } else if response == "cancelled" {
                sender_clone.input(PasswordDialogMsg::Cancel);
            }
        });

        let mut model = model;
        model.password_entry = Some(widgets.password_entry.clone());

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            PasswordDialogMsg::Show {
                index,
                filename,
                is_error,
                parent_window,
            } => {
                self.file_index = index;
                self.filename = filename;
                self.is_error = is_error;
                self.is_valid = false;
                root.present(Some(&parent_window));
                if let Some(entry) = &self.password_entry {
                    entry.set_text("");
                    entry.grab_focus();
                }
            }
            PasswordDialogMsg::PasswordChanged(pass) => {
                self.is_valid = !pass.is_empty();
            }
            PasswordDialogMsg::Unlock(pass) => {
                let _ = sender.output(PasswordDialogOutput::Unlock {
                    index: self.file_index.clone(),
                    password: pass,
                });
            }
            PasswordDialogMsg::Cancel => {
                let _ = sender.output(PasswordDialogOutput::Cancelled(self.file_index.clone()));
            }
        }
    }
}
