use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::gio;

use crate::tools::page::ToolPage;
use crate::tools::{Tool, open_pdf_dialog, select_folder_dialog};
use crate::pdf::{DivideAfter, SplitOptions, split_file, PdfError};

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
    prefix: String,
    prefix_changed: bool,
    divide_after: DivideAfter,
    is_splitting: bool,
}

#[derive(Debug)]
enum SplitPageMsg {
    AddFile(gio::File),
    SetDivideAfter(DivideAfter),
    SetPrefix(String),
    SplitTo(gio::File),
    SplitComplete(Result<(), PdfError>),
}

#[relm4::component]
impl Component for SplitPage {
    type Init = ();
    type Input = SplitPageMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        adw::ToastOverlay {
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
                        #[watch]
                        set_sensitive: !model.is_splitting,

                        connect_clicked[sender] => move |button| {
                            let sender_clone = sender.clone();
                            open_pdf_dialog(button, Tool::Split, move |mut files| {
                                if let Some(file) = files.pop() {
                                    sender_clone.input(SplitPageMsg::AddFile(file));
                                }
                            });
                        },
                    },

                    gtk::Button {
                        set_label: &gettext("Split"),
                        set_tooltip_text: Some(&gettext("Split")),
                        set_halign: gtk::Align::End,
                        add_css_class: "suggested-action",
                        #[watch]
                        set_sensitive: model.file.is_some() && !model.is_splitting,

                        connect_clicked[sender] => move |button| {
                            let sender_clone = sender.clone();
                            select_folder_dialog(button, &gettext("Select Output Folder"), move |folder| {
                                sender_clone.input(SplitPageMsg::SplitTo(folder));
                            });
                        }
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
                                connect_selected_notify[sender] => move |row| {
                                    let divide_after = match row.selected() {
                                        0 => DivideAfter::EachPage,
                                        1 => DivideAfter::EvenPages,
                                        _ => DivideAfter::OddPages,
                                    };
                                    sender.input(SplitPageMsg::SetDivideAfter(divide_after));
                                }
                            },

                            adw::EntryRow {
                                set_title: &gettext("Output prefix"),
                                #[track = "model.prefix_changed"]
                                set_text: &model.prefix,
                                set_show_apply_button: false,
                                connect_changed[sender] => move |entry| {
                                    sender.input(SplitPageMsg::SetPrefix(entry.text().to_string()));
                                }
                            }
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
        let model = Self {
            file: None,
            prefix: gettext("output_file"),
            prefix_changed: true,
            divide_after: DivideAfter::EachPage,
            is_splitting: false,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        self.prefix_changed = false;
        match message {
            SplitPageMsg::AddFile(file) => {
                self.prefix = file_stem(&file);
                self.prefix_changed = true;
                self.file = Some(file);
            }

            SplitPageMsg::SetDivideAfter(divide_after) => {
                self.divide_after = divide_after;
            }
            SplitPageMsg::SetPrefix(prefix) => {
                self.prefix = prefix;
            }
            SplitPageMsg::SplitTo(output_folder) => {
                if let (Some(file_path), Some(output_path)) = (self.file.as_ref().and_then(|f| f.path()), output_folder.path()) {
                    self.is_splitting = true;
                    let options = SplitOptions {
                        divide_after: self.divide_after,
                        prefix: self.prefix.clone(),
                    };
                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = split_file(&(file_path, 0), output_path, &options);
                        sender.input(SplitPageMsg::SplitComplete(result));
                    });
                }
            }
            SplitPageMsg::SplitComplete(result) => {
                self.is_splitting = false;
                match result {
                    Ok(_) => {
                        tracing::info!("Split PDF complete");
                        root.add_toast(adw::Toast::new(&gettext("Split PDFs saved")));
                    }
                    Err(err) => {
                        tracing::error!("Failed to split PDF: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to split PDF")));
                    }
                }
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
