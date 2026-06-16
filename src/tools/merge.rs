use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::gio;

use crate::tools::page::ToolPage;
use crate::tools::{Tool, files_from_model, pdf_dialog};

pub struct MergeTool {
    has_files: bool,
    _empty_page: Controller<ToolPage>,
    merge_page: Controller<MergePage>,
}

#[derive(Debug)]
pub enum MergeToolMsg {
    AddFiles(Vec<gio::File>),
    ClearFiles,
}

#[relm4::component(pub)]
impl SimpleComponent for MergeTool {
    type Init = ();
    type Input = MergeToolMsg;
    type Output = ();

    view! {
        gtk::Stack {
            #[watch]
            set_visible_child_name: if model.has_files { "merge" } else { "empty" },
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.merge_page.widget(), Some("merge")),
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let empty_page = ToolPage::builder()
            .launch(Tool::Merge)
            .forward(sender.input_sender(), MergeToolMsg::AddFiles);
        let merge_page = MergePage::builder()
            .launch(())
            .forward(sender.input_sender(), |_| MergeToolMsg::ClearFiles);

        let model = Self {
            has_files: false,
            _empty_page: empty_page,
            merge_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            MergeToolMsg::AddFiles(files) => {
                self.merge_page.emit(MergePageMsg::AddFiles(files));
                self.has_files = true;
            }
            MergeToolMsg::ClearFiles => {
                self.has_files = false;
            }
        }
    }
}

struct MergePage {
    files: FactoryVecDeque<MergeFileRow>,
}

#[derive(Debug)]
enum MergePageMsg {
    AddFiles(Vec<gio::File>),
    ClearFiles,
}

#[derive(Debug)]
enum MergePageOutput {
    ClearFiles,
}

#[relm4::component]
impl SimpleComponent for MergePage {
    type Init = ();
    type Input = MergePageMsg;
    type Output = MergePageOutput;

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
                    set_label: &Tool::Merge.action_label(),
                    set_tooltip_text: Some(&Tool::Merge.action_label()),
                    set_halign: gtk::Align::End,

                    connect_clicked[sender] => move |button| {
                        open_pdf_dialog(button, sender.clone());
                    },
                },

                gtk::Button {
                    set_label: &gettext("Clear"),
                    set_tooltip_text: Some(&gettext("Clear")),
                    set_halign: gtk::Align::End,

                    connect_clicked[sender] => move |_| {
                        sender.input(MergePageMsg::ClearFiles);
                    },
                },

                gtk::Button {
                    set_label: &gettext("Merge"),
                    set_tooltip_text: Some(&gettext("Merge")),
                    set_halign: gtk::Align::End,
                    add_css_class: "suggested-action"
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,

                #[local_ref]
                file_list -> gtk::ListBox {
                    add_css_class: "boxed-list",
                    set_selection_mode: gtk::SelectionMode::None,
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let files = FactoryVecDeque::builder().launch_default().detach();
        let model = Self { files };
        let file_list = model.files.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            MergePageMsg::AddFiles(files) => {
                let mut files_guard = self.files.guard();

                for file in files {
                    files_guard.push_back(file);
                }
            }
            MergePageMsg::ClearFiles => {
                {
                    let mut files_guard = self.files.guard();
                    files_guard.clear();
                }

                let _ = sender.output(MergePageOutput::ClearFiles);
            }
        }
    }
}

struct MergeFileRow {
    file: gio::File,
}

#[relm4::factory]
impl FactoryComponent for MergeFileRow {
    type Init = gio::File;
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow {
            set_title: &file_title(&self.file),
            set_title_lines: 1,
            set_activatable: true,

            add_prefix = &gtk::Image {
                set_icon_name: Some("text-x-generic-symbolic"),
            },

            add_suffix = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,

                append = &gtk::Button {
                    set_icon_name: "go-up-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center
                },

                append = &gtk::Button {
                    set_icon_name: "go-down-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center
                },

                append = &gtk::Button {
                    set_icon_name: "object-rotate-right-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center
                },

                append = &gtk::Button {
                    set_icon_name: "edit-delete-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center
                },
            }
        }
    }

    fn init_model(file: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { file }
    }
}

fn open_pdf_dialog(button: &gtk::Button, sender: ComponentSender<MergePage>) {
    let dialog = pdf_dialog(Tool::Merge);
    let parent = button.root().and_downcast::<gtk::Window>();

    dialog.open_multiple(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
        if let Ok(files) = result {
            sender.input(MergePageMsg::AddFiles(files_from_model(&files)));
        }
    });
}

fn file_title(file: &gio::File) -> String {
    file.basename()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.uri().to_string())
}
