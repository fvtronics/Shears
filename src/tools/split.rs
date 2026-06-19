use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::gio;

use crate::tools::page::ToolPage;
use crate::tools::{Tool, open_pdf_dialog};

pub struct SplitTool {
    has_file: bool,
    _empty_page: Controller<ToolPage>,
    split_page: Controller<SplitPage>,
}

#[derive(Debug)]
pub enum SplitToolMsg {
    AddFiles(Vec<gio::File>),
}

#[relm4::component(pub)]
impl SimpleComponent for SplitTool {
    type Init = ();
    type Input = SplitToolMsg;
    type Output = ();

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.split_page.widget(), Some("split")),

            #[watch]
            set_visible_child_name: if model.has_file { "split" } else { "empty" },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let empty_page = ToolPage::builder()
            .launch(Tool::Split)
            .forward(sender.input_sender(), SplitToolMsg::AddFiles);

        let split_page = SplitPage::builder().launch(()).detach();

        let model = Self {
            has_file: false,
            _empty_page: empty_page,
            split_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SplitToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.split_page.emit(SplitPageMsg::AddFile(file));
                    self.has_file = true;
                }
            }
        }
    }
}

struct SplitPage {
    file: Option<gio::File>,
}

#[derive(Debug)]
enum SplitPageMsg {
    AddFile(gio::File),
}

#[relm4::component]
impl SimpleComponent for SplitPage {
    type Init = ();
    type Input = SplitPageMsg;
    type Output = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_margin_all: 24,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,

                gtk::Box {
                    set_hexpand: true,
                },

                gtk::Button {
                    set_label: &Tool::Split.action_label(),
                    set_tooltip_text: Some(&Tool::Split.action_label()),
                    set_halign: gtk::Align::End,

                    connect_clicked[sender] => move |button| {
                        let sender_clone = sender.clone();
                        open_pdf_dialog(button, Tool::Split, move |mut files| {
                            if let Some(file) = files.pop() {
                                sender_clone.input(SplitPageMsg::AddFile(file));
                            }
                        });
                    },
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 24,
                set_margin_all: 24,

                gtk::AspectFrame {
                    set_ratio: 56.0 / 72.0,
                    set_obey_child: false,
                    set_hexpand: true,
                    set_valign: gtk::Align::Center,

                    #[wrap(Some)]
                    set_child = &gtk::Frame {
                        set_css_classes: &["view"]
                    }
                },

                gtk::ScrolledWindow {
                    set_width_request: 320,
                    set_vexpand: true,
                    set_hscrollbar_policy: gtk::PolicyType::Never,
                    set_propagate_natural_height: true,

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,

                        adw::PreferencesGroup {
                            adw::ComboRow {
                                set_title: &gettext("Divide after"),
                                set_model: Some(&gtk::StringList::new(&[
                                    gettext("Each page").as_str(),
                                    gettext("Even pages").as_str(),
                                    gettext("Odd pages").as_str(),
                                ])),
                            },

                            adw::EntryRow {
                                set_title: &gettext("Output prefix"),
                                #[watch]
                                set_text: &model.file.as_ref()
                                    .map(file_stem)
                                    .unwrap_or_else(|| gettext("output_file")),
                                set_show_apply_button: false,
                            }
                        }
                    }
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { file: None };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SplitPageMsg::AddFile(file) => {
                self.file = Some(file);
            }
        }
    }
}

fn file_stem(file: &gio::File) -> String {
    file.basename()
        .and_then(|name| {
            std::path::Path::new(&name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| file.uri().to_string())
}
