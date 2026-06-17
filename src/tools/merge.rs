use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio, glib};

use crate::modals::password::{PasswordDialog, PasswordDialogMsg, PasswordDialogOutput};
use crate::pdf::preview::PreviewError;
use crate::pdf::{MergeOptions, PdfError, merge_files};
use crate::tools::page::ToolPage;
use crate::tools::{Tool, files_from_model, pdf_dialog, save_pdf_dialog};

pub struct MergeTool {
    has_files: bool,
    file_count: usize,
    is_loading: bool,
    _empty_page: Controller<ToolPage>,
    merge_page: Controller<MergePage>,
}

#[derive(Debug)]
pub enum MergeToolMsg {
    AddFiles(Vec<gio::File>),
    ClearFiles,
    UpdateFileCount(usize),
    Loading(bool),
}

#[derive(Debug)]
pub enum MergeToolOutput {
    FileCountChanged(usize),
    Loading(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for MergeTool {
    type Init = ();
    type Input = MergeToolMsg;
    type Output = MergeToolOutput;

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
            .forward(sender.input_sender(), |msg| match msg {
                MergePageOutput::ClearFiles => MergeToolMsg::ClearFiles,
                MergePageOutput::FileCountChanged(len) => MergeToolMsg::UpdateFileCount(len),
                MergePageOutput::Loading(is_loading) => MergeToolMsg::Loading(is_loading),
            });

        let model = Self {
            has_files: false,
            file_count: 0,
            is_loading: false,
            _empty_page: empty_page,
            merge_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            MergeToolMsg::AddFiles(files) => {
                self.merge_page.emit(MergePageMsg::AddFiles(files));
            }
            MergeToolMsg::ClearFiles => {
                self.has_files = false;
                self.file_count = 0;
            }
            MergeToolMsg::UpdateFileCount(len) => {
                self.file_count = len;
                if len == 0 {
                    self.has_files = false;
                }

                if len > 0 && !self.is_loading {
                    self.has_files = true;
                }
                let _ = sender.output(MergeToolOutput::FileCountChanged(len));
            }
            MergeToolMsg::Loading(is_loading) => {
                self.is_loading = is_loading;
                self._empty_page.emit(is_loading);
                if !is_loading && self.file_count > 0 {
                    self.has_files = true;
                }
                let _ = sender.output(MergeToolOutput::Loading(is_loading));
            }
        }
    }
}

struct MergePage {
    files: FactoryVecDeque<MergeFileRow>,
    modern_pdf_format: bool,
    normalize_page_size: bool,
    remove_metadata: bool,
    output_file: Option<String>,
    is_loading: bool,
    password_dialog: Controller<PasswordDialog>,
    password_queue: std::collections::VecDeque<PasswordRequest>,
}

struct PasswordRequest {
    index: DynamicIndex,
    filename: String,
    is_error: bool,
}

#[derive(Debug, Clone)]
pub struct PreparedFile {
    pub file: gio::File,
    pub title: String,
    pub size_str: String,
}

#[derive(Debug)]
enum MergePageMsg {
    AddFiles(Vec<gio::File>),
    FilesReady(Vec<PreparedFile>),
    AddFilesBatch(Vec<PreparedFile>),
    ClearFiles,
    MoveFileUp(DynamicIndex),
    MoveFileDown(DynamicIndex),
    MoveFile {
        from: usize,
        to: DynamicIndex,
    },
    DeleteFile(DynamicIndex),
    SetModernPdfFormat(bool),
    SetNormalizePageSize(bool),
    SetRemoveMetadata(bool),
    RotateAll,
    MergeTo(gio::File),
    MergeComplete(Result<std::path::PathBuf, PdfError>),
    OpenOutput,
    LoadingComplete,
    PasswordRequired {
        index: DynamicIndex,
        filename: String,
        is_error: bool,
    },
    PasswordSuccess(DynamicIndex),
    PasswordDialogOutput(PasswordDialogOutput),
}

#[derive(Debug)]
pub enum MergePageOutput {
    ClearFiles,
    FileCountChanged(usize),
    Loading(bool),
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
                #[watch]
                set_sensitive: !model.is_loading,

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
                    set_label: &gettext("Open Output"),
                    #[watch]
                    set_visible: model.output_file.is_some(),
                    set_tooltip_text: Some(&gettext("Open Output File")),

                    connect_clicked[sender] => move |_| {
                        sender.input(MergePageMsg::OpenOutput);
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
                    #[watch]
                    set_sensitive: !model.is_loading,
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
                    MergeFileRowOutput::PasswordRequired {
                        index,
                        filename,
                        is_error,
                    } => MergePageMsg::PasswordRequired {
                        index,
                        filename,
                        is_error,
                    },
                    MergeFileRowOutput::PasswordSuccess(index) => {
                        MergePageMsg::PasswordSuccess(index)
                    }
                });
        let password_dialog = PasswordDialog::builder()
            .launch(())
            .forward(sender.input_sender(), MergePageMsg::PasswordDialogOutput);
        let model = Self {
            files,
            modern_pdf_format: false,
            normalize_page_size: false,
            remove_metadata: false,
            output_file: None,
            is_loading: false,
            password_dialog,
            password_queue: std::collections::VecDeque::new(),
        };
        let file_list = model.files.widget();
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            MergePageMsg::MergeTo(output_file) => {
                self.is_loading = true;
                let _ = sender.output(MergePageOutput::Loading(true));

                let files: Vec<(std::path::PathBuf, u16, Option<String>)> = self
                    .files
                    .guard()
                    .iter()
                    .filter_map(|row| {
                        row.file
                            .path()
                            .map(|p| (p, row.rotation, row.password.clone()))
                    })
                    .collect();

                let options = MergeOptions {
                    modern_format: self.modern_pdf_format,
                    normalize_page_size: self.normalize_page_size,
                    remove_metadata: self.remove_metadata,
                };

                if let Some(output_path) = output_file.path() {
                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = merge_files(&files, output_path.clone(), &options);
                        let msg_result = result.map(|_| output_path);
                        sender.input(MergePageMsg::MergeComplete(msg_result));
                    });
                }
            }
            MergePageMsg::MergeComplete(result) => {
                self.is_loading = false;
                let _ = sender.output(MergePageOutput::Loading(false));

                match result {
                    Ok(path) => {
                        self.output_file = Some(path.to_string_lossy().into_owned());
                        let toast = adw::Toast::new(&gettext("PDFs merged successfully"));
                        root.add_toast(toast);
                        tracing::info!("Merged PDF Saved");
                    }
                    Err(err) => {
                        let toast = adw::Toast::new(&gettext("Could not save PDF"));
                        root.add_toast(toast);
                        tracing::error!("Failed to merge PDFs: {:?}", err);
                    }
                }
            }
            MergePageMsg::OpenOutput => {
                if let Some(path_str) = self.output_file.clone() {
                    let file = gio::File::for_path(&path_str);
                    if !file.query_exists(gio::Cancellable::NONE) {
                        let toast = adw::Toast::new(&gettext("Output file not found"));
                        root.add_toast(toast);
                        self.output_file = None;
                        tracing::error!("Output file no longer exists at: {}", path_str);
                    } else if let Err(e) = gio::AppInfo::launch_default_for_uri(
                        file.uri().as_str(),
                        None::<&gio::AppLaunchContext>,
                    ) {
                        let toast = adw::Toast::new(&gettext("Failed to open output file"));
                        root.add_toast(toast);
                        tracing::error!("Failed to open output file: {:?}", e);
                    }
                }
            }
            MergePageMsg::AddFiles(files) => {
                self.is_loading = true;
                let _ = sender.output(MergePageOutput::Loading(true));

                let sender_clone = sender.clone();
                relm4::spawn_blocking(move || {
                    let prepared: Vec<PreparedFile> = files
                        .into_iter()
                        .map(|file| {
                            let title = file_title(&file);
                            let size_str = file_size_string(&file);
                            PreparedFile {
                                file,
                                title,
                                size_str,
                            }
                        })
                        .collect();
                    sender_clone.input(MergePageMsg::FilesReady(prepared));
                });
            }
            MergePageMsg::FilesReady(prepared_files) => {
                let sender_clone = sender.clone();
                relm4::spawn_local(async move {
                    for chunk in prepared_files.chunks(20) {
                        sender_clone.input(MergePageMsg::AddFilesBatch(chunk.to_vec()));
                        relm4::gtk::glib::timeout_future(std::time::Duration::from_millis(5)).await;
                    }
                    sender_clone.input(MergePageMsg::LoadingComplete);
                });
            }
            MergePageMsg::AddFilesBatch(files) => {
                let mut files_guard = self.files.guard();
                for file in files {
                    files_guard.push_back(file);
                }
                let len = files_guard.len();
                drop(files_guard);
                let _ = sender.output(MergePageOutput::FileCountChanged(len));
                self.update_bounds();
            }
            MergePageMsg::LoadingComplete => {
                self.is_loading = false;
                let _ = sender.output(MergePageOutput::Loading(false));
            }
            MergePageMsg::ClearFiles => {
                self.output_file = None;
                {
                    let mut files_guard = self.files.guard();
                    files_guard.clear();
                }

                let _ = sender.output(MergePageOutput::FileCountChanged(0));
                let _ = sender.output(MergePageOutput::ClearFiles);
            }
            MergePageMsg::MoveFileUp(index) => {
                let index = index.current_index();

                if index != 0 {
                    self.files.guard().move_to(index, index - 1);
                    self.update_bounds();
                }
            }
            MergePageMsg::MoveFileDown(index) => {
                let index = index.current_index();
                let new_index = index + 1;

                if new_index < self.files.len() {
                    self.files.guard().move_to(index, new_index);
                    self.update_bounds();
                }
            }
            MergePageMsg::MoveFile { from, to } => {
                let to = to.current_index();
                if from != to {
                    self.files.guard().move_to(from, to);
                    self.update_bounds();
                }
            }
            MergePageMsg::DeleteFile(index) => {
                let current_index = index.current_index();

                if let Some(pos) = self
                    .password_queue
                    .iter()
                    .position(|req| req.index == index)
                {
                    self.password_queue.remove(pos);
                    if pos == 0 {
                        self.process_password_queue(root);
                    }
                }

                self.files.guard().remove(current_index);
                let len = self.files.len();
                let _ = sender.output(MergePageOutput::FileCountChanged(len));
                self.update_bounds();
            }
            MergePageMsg::SetModernPdfFormat(active) => {
                self.modern_pdf_format = active;
                self.output_file = None;
            }
            MergePageMsg::SetNormalizePageSize(active) => {
                self.normalize_page_size = active;
                self.output_file = None;
            }
            MergePageMsg::SetRemoveMetadata(active) => {
                self.remove_metadata = active;
                self.output_file = None;
            }
            MergePageMsg::RotateAll => {
                self.output_file = None;
                for i in 0..self.files.len() {
                    self.files.send(i, MergeFileRowMsg::RotateClockwise);
                }
            }
            MergePageMsg::PasswordRequired {
                index,
                filename,
                is_error,
            } => {
                if let Some(pos) = self
                    .password_queue
                    .iter()
                    .position(|req| req.index == index)
                {
                    self.password_queue[pos].is_error = is_error;
                    if pos == 0 {
                        self.process_password_queue(root);
                    }
                } else {
                    self.password_queue.push_back(PasswordRequest {
                        index,
                        filename,
                        is_error,
                    });
                    if self.password_queue.len() == 1 {
                        self.process_password_queue(root);
                    }
                }
            }
            MergePageMsg::PasswordSuccess(index) => {
                if let Some(pos) = self
                    .password_queue
                    .iter()
                    .position(|req| req.index == index)
                {
                    self.password_queue.remove(pos);
                    if pos == 0 {
                        self.process_password_queue(root);
                    }
                }
            }
            MergePageMsg::PasswordDialogOutput(output) => match output {
                PasswordDialogOutput::Unlock { index, password } => {
                    self.files.send(
                        index.current_index(),
                        MergeFileRowMsg::RetryWithPassword(password),
                    );
                }
                PasswordDialogOutput::Cancelled(index) => {
                    sender.input(MergePageMsg::DeleteFile(index));
                }
            },
        }
    }
}

impl MergePage {
    fn process_password_queue(&mut self, root: &<Self as Component>::Root) {
        if let Some(req) = self.password_queue.front()
            && let Some(window) = root.root().and_downcast::<gtk::Window>()
        {
            self.password_dialog.emit(PasswordDialogMsg::Show {
                index: req.index.clone(),
                filename: req.filename.clone(),
                is_error: req.is_error,
                parent_window: window,
            });
        }
    }

    fn update_bounds(&mut self) {
        self.output_file = None;
        let length = self.files.len();
        let files_guard = self.files.guard();
        for i in 0..length {
            let is_first = i == 0;
            let is_last = i == length - 1;
            if let Some(row) = files_guard.get(i)
                && (row.is_first != is_first || row.is_last != is_last)
            {
                files_guard.send(i, MergeFileRowMsg::UpdateBounds { is_first, is_last });
            }
        }
    }
}

struct MergeFileRow {
    file: gio::File,
    title: String,
    size_str: String,
    rotation: u16,
    is_first: bool,
    is_last: bool,
    thumbnail: Option<gdk::MemoryTexture>,
    password: Option<String>,
    index: DynamicIndex,
}

#[derive(Debug)]
enum MergeFileRowMsg {
    RotateClockwise,
    UpdateBounds { is_first: bool, is_last: bool },
    ThumbnailReady(Result<Option<gdk::MemoryTexture>, PreviewError>),
    RetryWithPassword(String),
}

#[derive(Debug)]
enum MergeFileRowOutput {
    MoveUp(DynamicIndex),
    MoveDown(DynamicIndex),
    Delete(DynamicIndex),
    Move {
        from: usize,
        to: DynamicIndex,
    },
    PasswordRequired {
        index: DynamicIndex,
        filename: String,
        is_error: bool,
    },
    PasswordSuccess(DynamicIndex),
}

#[relm4::factory]
impl FactoryComponent for MergeFileRow {
    type Init = PreparedFile;
    type Input = MergeFileRowMsg;
    type Output = MergeFileRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        adw::ActionRow {
            set_title: &self.title,
            set_subtitle: &self.size_str,
            set_title_lines: 1,
            set_activatable: true,

            #[name(preview_frame)]
            add_prefix = &gtk::Overlay {
                set_margin_top: 6,
                set_margin_bottom: 6,
                set_valign: gtk::Align::Center,
                set_vexpand: false,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_width_request: 56,
                    set_height_request: 72,
                },
                add_overlay = &gtk::Picture {
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Contain,
                    #[watch]
                    set_paintable: self.thumbnail.as_ref(),
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
                    set_sensitive: !self.is_first,

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
                    set_sensitive: !(self.is_first && self.is_last),

                    connect_clicked[sender, index] => move |_| {
                        let _ = sender.output(MergeFileRowOutput::Delete(index.clone()));
                    }
                },
            }
        }
    }

    fn init_model(prepared: Self::Init, index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let file_clone = prepared.file.clone();
        let sender_clone = sender.clone();

        let rotation = 0;

        request_thumbnail(file_clone, rotation, None, sender_clone);

        Self {
            file: prepared.file,
            title: prepared.title,
            size_str: prepared.size_str,
            rotation: 0,
            is_first: index.current_index() == 0,
            is_last: false,
            thumbnail: None,
            password: None,
            index: index.clone(),
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            MergeFileRowMsg::RotateClockwise => {
                self.rotation = (self.rotation + 90) % 360;

                let file_clone = self.file.clone();
                let rotation = self.rotation;
                let sender_clone = sender.clone();
                let password = self.password.clone();

                request_thumbnail(file_clone, rotation, password, sender_clone);
            }
            MergeFileRowMsg::UpdateBounds { is_first, is_last } => {
                self.is_first = is_first;
                self.is_last = is_last;
            }
            MergeFileRowMsg::ThumbnailReady(result) => match result {
                Ok(texture) => {
                    self.thumbnail = texture;
                    if self.password.is_some() {
                        let _ =
                            sender.output(MergeFileRowOutput::PasswordSuccess(self.index.clone()));
                    }
                }
                Err(PreviewError::Encrypted) => {
                    let is_error = self.password.is_some();
                    let _ = sender.output(MergeFileRowOutput::PasswordRequired {
                        index: self.index.clone(),
                        filename: self.title.clone(),
                        is_error,
                    });
                }
                Err(_) => {
                    if self.password.is_some() {
                        let _ =
                            sender.output(MergeFileRowOutput::PasswordSuccess(self.index.clone()));
                    }
                }
            },
            MergeFileRowMsg::RetryWithPassword(password) => {
                self.password = Some(password.clone());

                let file_clone = self.file.clone();
                let rotation = self.rotation;
                let sender_clone = sender.clone();

                request_thumbnail(file_clone, rotation, Some(password), sender_clone);
            }
        }
    }
}

fn request_thumbnail(
    file: gio::File,
    rotation: u16,
    password: Option<String>,
    sender: FactorySender<MergeFileRow>,
) {
    crate::pdf::preview::thread_pool()
        .push(move || {
            let result = crate::pdf::preview::generate_thumbnail(
                &file,
                rotation as i32,
                password.as_deref(),
            );
            sender.input(MergeFileRowMsg::ThumbnailReady(result));
        })
        .expect("Failed to enqueue thumbnail task");
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
    file.query_info(
        "standard::size",
        gio::FileQueryInfoFlags::NONE,
        gio::Cancellable::NONE,
    )
    .map(|info| glib::format_size(info.size() as u64).to_string())
    .unwrap_or_default()
}
