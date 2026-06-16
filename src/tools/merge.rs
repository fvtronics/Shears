use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio, glib};

use crate::pdf::{MergeOptions, PdfError, merge_files};
use crate::tools::page::ToolPage;
use crate::tools::{Tool, files_from_model, pdf_dialog, save_pdf_dialog};

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
    modern_pdf_format: bool,
    normalize_page_size: bool,
    remove_metadata: bool,
}

#[derive(Debug)]
enum MergePageMsg {
    AddFiles(Vec<gio::File>),
    ClearFiles,
    MoveFileUp(DynamicIndex),
    MoveFileDown(DynamicIndex),
    MoveFile { from: usize, to: DynamicIndex },
    DeleteFile(DynamicIndex),
    SetModernPdfFormat(bool),
    SetNormalizePageSize(bool),
    SetRemoveMetadata(bool),
    RotateAll,
    MergeTo(gio::File),
    MergeComplete(Result<(), PdfError>),
}

#[derive(Debug)]
enum MergePageOutput {
    ClearFiles,
}

#[relm4::component]
impl Component for MergePage {
    type Init = ();
    type Input = MergePageMsg;
    type Output = MergePageOutput;
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
                set_spacing: 8,

                gtk::Box {
                    set_hexpand: true,
                },

                gtk::Button {
                    set_label: &Tool::Merge.action_label(),
                    set_tooltip_text: Some(&gettext("Add PDF Files")),

                    connect_clicked[sender] => move |button| {
                        open_pdf_dialog(button, sender.clone());
                    },
                },

                gtk::Button {
                    set_label: &gettext("Clear"),
                    set_tooltip_text: Some(&gettext("Clear File List")),

                    connect_clicked[sender] => move |_| {
                        sender.input(MergePageMsg::ClearFiles);
                    },
                },

                gtk::Button {
                    set_label: &gettext("Merge"),
                    set_tooltip_text: Some(&gettext("Merge Selected PDFs")),
                    add_css_class: "suggested-action",

                    connect_clicked[sender] => move |button| {
                        let sender_clone = sender.clone();
                        save_pdf_dialog(button, Tool::Merge, &gettext("Save Merged PDF"), move |file| {
                            sender_clone.input(MergePageMsg::MergeTo(file));
                        });
                    }
                },

                gtk::MenuButton {
                    set_icon_name: "view-more-symbolic",
                    add_css_class: "flat",
                    set_tooltip_text: Some(&gettext("Advanced Options")),

                    #[wrap(Some)]
                    set_popover = &gtk::Popover {
                        add_css_class: "menu",
                        adw::PreferencesGroup {
                            add = &adw::ActionRow {
                                set_title: &gettext("Rotate all"),
                                set_activatable: true,

                                connect_activated[sender] => move |_| {
                                    sender.input(MergePageMsg::RotateAll);
                                }
                            },

                            add = &adw::SwitchRow {
                                set_title: &gettext("Modern PDF format"),
                                set_subtitle: &gettext("Save with PDF 1.5 object streams"),
                                set_active: model.modern_pdf_format,

                                connect_active_notify[sender] => move |row| {
                                    sender.input(MergePageMsg::SetModernPdfFormat(row.is_active()));
                                }
                            },

                            add = &adw::SwitchRow {
                                set_title: &gettext("Normalize page size"),
                                set_subtitle: &gettext("Resize output pages to the largest page size"),
                                set_active: model.normalize_page_size,

                                connect_active_notify[sender] => move |row| {
                                    sender.input(MergePageMsg::SetNormalizePageSize(row.is_active()));
                                }
                            },

                            add = &adw::SwitchRow {
                                set_title: &gettext("Remove metadata"),
                                set_subtitle: &gettext("Remove existing metadata before saving"),
                                set_active: model.remove_metadata,

                                connect_active_notify[sender] => move |row| {
                                    sender.input(MergePageMsg::SetRemoveMetadata(row.is_active()));
                                }
                            },
                        }
                    }
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
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let files =
            FactoryVecDeque::builder()
                .launch_default()
                .forward(sender.input_sender(), |output| match output {
                    MergeFileRowOutput::MoveUp(index) => MergePageMsg::MoveFileUp(index),
                    MergeFileRowOutput::MoveDown(index) => MergePageMsg::MoveFileDown(index),
                    MergeFileRowOutput::Delete(index) => MergePageMsg::DeleteFile(index),
                    MergeFileRowOutput::Move { from, to } => MergePageMsg::MoveFile { from, to },
                });
        let model = Self {
            files,
            modern_pdf_format: false,
            normalize_page_size: false,
            remove_metadata: false,
        };
        let file_list = model.files.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            MergePageMsg::MergeTo(output_file) => {
                let files: Vec<(std::path::PathBuf, u16)> = self
                    .files
                    .guard()
                    .iter()
                    .filter_map(|row| row.file.path().map(|p| (p, row.rotation)))
                    .collect();

                let options = MergeOptions {
                    modern_format: self.modern_pdf_format,
                    normalize_page_size: self.normalize_page_size,
                    remove_metadata: self.remove_metadata,
                };

                if let Some(output_path) = output_file.path() {
                    let sender = sender.clone();
                    std::thread::spawn(move || {
                        let result = merge_files(&files, output_path, &options);
                        sender.input(MergePageMsg::MergeComplete(result));
                    });
                }
            }
            MergePageMsg::MergeComplete(result) => match result {
                Ok(_) => {
                    let toast = adw::Toast::new(&gettext("PDFs merged successfully"));
                    root.add_toast(toast);
                    tracing::info!("Merged PDF Saved");
                }
                Err(err) => {
                    let toast = adw::Toast::new(&gettext("Could not save PDF"));
                    root.add_toast(toast);
                    tracing::error!("Failed to merge PDFs: {:?}", err);
                }
            },
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
            MergePageMsg::MoveFileUp(index) => {
                let index = index.current_index();

                if index != 0 {
                    self.files.guard().move_to(index, index - 1);
                }
            }
            MergePageMsg::MoveFileDown(index) => {
                let index = index.current_index();
                let new_index = index + 1;

                if new_index < self.files.len() {
                    self.files.guard().move_to(index, new_index);
                }
            }
            MergePageMsg::MoveFile { from, to } => {
                let to = to.current_index();
                if from != to {
                    self.files.guard().move_to(from, to);
                }
            }
            MergePageMsg::DeleteFile(index) => {
                self.files.guard().remove(index.current_index());
            }
            MergePageMsg::SetModernPdfFormat(active) => {
                self.modern_pdf_format = active;
            }
            MergePageMsg::SetNormalizePageSize(active) => {
                self.normalize_page_size = active;
            }
            MergePageMsg::SetRemoveMetadata(active) => {
                self.remove_metadata = active;
            }
            MergePageMsg::RotateAll => {
                for i in 0..self.files.len() {
                    self.files.send(i, MergeFileRowMsg::RotateClockwise);
                }
            }
        }

        let length = self.files.len();
        for i in 0..length {
            let is_last = i == length - 1;
            self.files
                .send(i, MergeFileRowMsg::UpdateBounds { is_last });
        }
    }
}

struct MergeFileRow {
    file: gio::File,
    rotation: u16,
    is_last: bool,
    index: DynamicIndex,
}

#[derive(Debug)]
enum MergeFileRowMsg {
    RotateClockwise,
    UpdateBounds { is_last: bool },
}

#[derive(Debug)]
enum MergeFileRowOutput {
    MoveUp(DynamicIndex),
    MoveDown(DynamicIndex),
    Delete(DynamicIndex),
    Move { from: usize, to: DynamicIndex },
}

#[relm4::factory]
impl FactoryComponent for MergeFileRow {
    type Init = gio::File;
    type Input = MergeFileRowMsg;
    type Output = MergeFileRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow {
            set_title: &file_title(&self.file),
            set_subtitle: &file_size_string(&self.file),
            set_title_lines: 1,
            set_activatable: true,

            #[name(preview_frame)]
            add_prefix = &gtk::Frame {
                set_width_request: 56,
                set_height_request: 72,
                set_margin_top: 6,
                set_margin_bottom: 6,
                set_valign: gtk::Align::Center,
                set_vexpand: false,

                #[wrap(Some)]
                set_child = &gtk::Picture {
                    set_content_fit: gtk::ContentFit::Contain,
                }
            },

            add_controller = gtk::DragSource {
                set_actions: gdk::DragAction::MOVE,

                connect_prepare[index] => move |_drag_source, _x, _y| {
                    let current = index.current_index() as u32;
                    let value = current.to_value();
                    Some(gdk::ContentProvider::for_value(&value))
                },

                connect_drag_begin[preview_frame] => move |_, drag| {
                    let paintable = gtk::WidgetPaintable::new(Some(&preview_frame));
                    gtk::DragIcon::set_from_paintable(drag, &paintable, 0, 0);
                }
            },

            add_controller = gtk::DropTarget::new(u32::static_type(), gdk::DragAction::MOVE) {
                connect_drop[sender, index] => move |_drop_target, value, _x, _y| {
                    if let Ok(from_index) = value.get::<u32>() {
                        let _ = sender.output(MergeFileRowOutput::Move {
                            from: from_index as usize,
                            to: index.clone(),
                        });
                        true
                    } else {
                        false
                    }
                }
            },

            add_suffix = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,

                append = &gtk::Button {
                    set_icon_name: "go-up-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some(&gettext("Move Up")),
                    #[watch]
                    set_sensitive: self.index.current_index() != 0,

                    connect_clicked[sender, index] => move |_| {
                        let _ = sender.output(MergeFileRowOutput::MoveUp(index.clone()));
                    }
                },

                append = &gtk::Button {
                    set_icon_name: "go-down-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some(&gettext("Move Down")),
                    #[watch]
                    set_sensitive: !self.is_last,

                    connect_clicked[sender, index] => move |_| {
                        let _ = sender.output(MergeFileRowOutput::MoveDown(index.clone()));
                    }
                },

                append = &gtk::Button {
                    set_icon_name: "object-rotate-right-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some(&gettext("Rotate Clockwise")),

                    connect_clicked => MergeFileRowMsg::RotateClockwise
                },

                append = &gtk::Button {
                    set_icon_name: "edit-delete-symbolic",
                    add_css_class: "flat",
                    set_vexpand: false,
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some(&gettext("Remove File")),
                    #[watch]
                    set_sensitive: !(self.index.current_index() == 0 && self.is_last),

                    connect_clicked[sender, index] => move |_| {
                        let _ = sender.output(MergeFileRowOutput::Delete(index.clone()));
                    }
                },
            }
        }
    }

    fn init_model(file: Self::Init, index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self {
            file,
            rotation: 0,
            is_last: false,
            index: index.clone(),
        }
    }

    fn update(&mut self, message: Self::Input, _sender: FactorySender<Self>) {
        match message {
            MergeFileRowMsg::RotateClockwise => {
                self.rotation = (self.rotation + 90) % 360;
            }
            MergeFileRowMsg::UpdateBounds { is_last } => {
                self.is_last = is_last;
            }
        }
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

fn file_size_string(file: &gio::File) -> String {
    match file.query_info(
        "standard::size",
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    ) {
        Ok(info) => glib::format_size(info.size() as u64).to_string(),
        Err(_) => String::new(),
    }
}
