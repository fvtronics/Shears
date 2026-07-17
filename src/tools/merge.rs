use gettextrs::{gettext, ngettext};
use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio, glib};

use crate::modals::password::{
    PasswordDialog, PasswordDialogMsg, PasswordDialogOutput, SecretString,
};
use crate::pdf::preview::PreviewError;
use crate::pdf::{MergeOptions, PdfError, merge_files};
use crate::tools::page::ToolPage;
use crate::tools::{
    PreviewStatus, Tool, ToolOutput, confirm_dialog, open_pdf_dialog, save_pdf_dialog,
};

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

#[relm4::component(pub)]
impl SimpleComponent for MergeTool {
    type Init = ();
    type Input = MergeToolMsg;
    type Output = ToolOutput;

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.merge_page.widget(), Some("merge")),

            #[watch]
            set_visible_child_name: if model.has_files { "merge" } else { "empty" },
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
                let _ = sender.output(ToolOutput::Subtitle(None));
            }
            MergeToolMsg::UpdateFileCount(len) => {
                self.file_count = len;
                if len == 0 {
                    self.has_files = false;
                    let _ = sender.output(ToolOutput::Subtitle(None));
                } else {
                    if !self.is_loading {
                        self.has_files = true;
                    }
                    let count = len as u32;
                    let subtitle =
                        ngettext("{count} file selected", "{count} files selected", count)
                            .replace("{count}", &count.to_string());
                    let _ = sender.output(ToolOutput::Subtitle(Some(subtitle)));
                }
            }
            MergeToolMsg::Loading(is_loading) => {
                self.is_loading = is_loading;
                self._empty_page.emit(is_loading);
                if !is_loading && self.file_count > 0 {
                    self.has_files = true;
                }
                let _ = sender.output(ToolOutput::Loading(is_loading));
            }
        }
    }
}

struct MergePage {
    files: FactoryVecDeque<MergeFileRow>,
    modern_pdf_format: bool,
    normalize_page_size: bool,
    remove_metadata: bool,
    is_loading: bool,
    is_adding_files: bool,
    is_merging: bool,
    password_dialog: Controller<PasswordDialog>,
    password_queue: std::collections::VecDeque<PasswordRequest>,
}

struct PasswordRequest {
    index: DynamicIndex,
    filename: String,
    is_error: bool,
}

#[derive(Debug, Clone)]
pub enum MergeItemType {
    File(gio::File),
    BlankPage { width: f64, height: f64 },
}

#[derive(Debug, Clone)]
pub struct PreparedFile {
    pub item_type: MergeItemType,
    pub title: String,
    pub size_str: String,
    pub rotation: u16,
    pub password: Option<SecretString>,
    pub thumbnail: Option<gdk::MemoryTexture>,
    pub original_dimensions: Option<(f64, f64)>,
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
    DeleteFile {
        index: DynamicIndex,
        show_toast: bool,
    },
    UndoDeleteFile {
        index: usize,
        prepared: PreparedFile,
    },
    DuplicateFile(DynamicIndex),
    SetModernPdfFormat(bool),
    SetNormalizePageSize(bool),
    SetRemoveMetadata(bool),
    RotateAll,
    MergeTo(gio::File),
    MergeComplete(Result<std::path::PathBuf, PdfError>),
    OpenOutput(std::path::PathBuf),
    LoadingComplete,
    PasswordRequired {
        index: DynamicIndex,
        filename: String,
        is_error: bool,
    },
    PasswordSuccess(DynamicIndex),
    PasswordDialogOutput(PasswordDialogOutput),
    PreviewComplete,
    InsertBlankPageAfter {
        index: DynamicIndex,
        width: f64,
        height: f64,
        rotation: u16,
    },
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
                set_halign: gtk::Align::End,
                #[watch]
                set_sensitive: !model.is_loading,

                gtk::Button {
                    set_label: &Tool::Merge.action_label(),
                    set_tooltip_text: Some(&gettext("Add PDF Files")),
                    set_can_shrink: true,

                    connect_clicked[sender] => move |button| {
                        let sender_clone = sender.clone();
                        open_pdf_dialog(button, Tool::Merge, move |files| {
                            sender_clone.input(MergePageMsg::AddFiles(files));
                        });
                    },
                },

                gtk::Button {
                    set_label: &gettext("Clear"),
                    set_tooltip_text: Some(&gettext("Clear File List")),
                    set_can_shrink: true,

                    connect_clicked[sender] => move |button| {
                        let sender_clone = sender.clone();
                        confirm_dialog(
                            button,
                            &gettext("Clear File List?"),
                            &gettext("All selected PDF files will be removed from the list."),
                            &gettext("Clear"),
                            adw::ResponseAppearance::Destructive,
                            move || {
                                sender_clone.input(MergePageMsg::ClearFiles);
                            },
                        );
                    },
                },

                gtk::Button {
                    set_label: &gettext("Merge"),
                    set_tooltip_text: Some(&gettext("Merge Selected PDFs")),
                    add_css_class: "suggested-action",
                    set_can_shrink: true,

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
                                set_title: &gettext("Rotate _all"),
                                set_use_underline: true,
                                set_activatable: true,

                                connect_activated[sender] => move |_| {
                                    sender.input(MergePageMsg::RotateAll);
                                }
                            },

                            add = &adw::SwitchRow {
                                set_title: &gettext("_Modern PDF format"),
                                set_use_underline: true,
                                set_subtitle: &gettext("Save with PDF 1.5 object streams"),
                                set_active: model.modern_pdf_format,

                                connect_active_notify[sender] => move |row| {
                                    sender.input(MergePageMsg::SetModernPdfFormat(row.is_active()));
                                }
                            },

                            add = &adw::SwitchRow {
                                set_title: &gettext("_Normalize page size"),
                                set_use_underline: true,
                                set_subtitle: &gettext("Resize output pages to the largest page size"),
                                set_active: model.normalize_page_size,

                                connect_active_notify[sender] => move |row| {
                                    sender.input(MergePageMsg::SetNormalizePageSize(row.is_active()));
                                }
                            },

                            add = &adw::SwitchRow {
                                set_title: &gettext("_Remove metadata"),
                                set_use_underline: true,
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
                    MergeFileRowOutput::Delete(index) => MergePageMsg::DeleteFile {
                        index,
                        show_toast: true,
                    },
                    MergeFileRowOutput::Duplicate(index) => MergePageMsg::DuplicateFile(index),
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
                    MergeFileRowOutput::PreviewComplete => MergePageMsg::PreviewComplete,
                    MergeFileRowOutput::InsertBlankPageAfter {
                        index,
                        width,
                        height,
                        rotation,
                    } => MergePageMsg::InsertBlankPageAfter {
                        index,
                        width,
                        height,
                        rotation,
                    },
                });
        let password_dialog = PasswordDialog::builder()
            .launch(())
            .forward(sender.input_sender(), MergePageMsg::PasswordDialogOutput);
        let model = Self {
            files,
            modern_pdf_format: false,
            normalize_page_size: false,
            remove_metadata: false,
            is_loading: false,
            is_adding_files: false,
            is_merging: false,
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
                self.is_merging = true;
                self.check_loading_state(&sender);

                let files: Vec<(crate::pdf::merge::MergeInput, u16)> = self
                    .files
                    .guard()
                    .iter()
                    .filter_map(|row| match &row.item_type {
                        MergeItemType::File(file) => file.path().map(|p| {
                            (
                                crate::pdf::merge::MergeInput::File(
                                    p,
                                    row.password.as_ref().map(|s| s.0.clone()),
                                ),
                                row.rotation,
                            )
                        }),
                        MergeItemType::BlankPage { width, height } => Some((
                            crate::pdf::merge::MergeInput::BlankPage {
                                title: row.title.clone(),
                                width: *width,
                                height: *height,
                            },
                            row.rotation,
                        )),
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
                self.is_merging = false;
                self.check_loading_state(&sender);

                match result {
                    Ok(path) => {
                        tracing::info!("Merged PDF Saved");
                        let toast = adw::Toast::new(&gettext("PDFs merged successfully"));
                        toast.set_button_label(Some(&gettext("Open File")));
                        let sender_clone = sender.clone();
                        toast.connect_button_clicked(move |_| {
                            sender_clone.input(MergePageMsg::OpenOutput(path.clone()));
                        });
                        root.add_toast(toast);
                    }
                    Err(err) => {
                        let toast = adw::Toast::new(&gettext("Could not save PDF"));
                        root.add_toast(toast);
                        tracing::error!("Failed to merge PDFs: {:?}", err);
                    }
                }
            }
            MergePageMsg::OpenOutput(path) => {
                let file = gio::File::for_path(&path);
                if let Err(e) = gio::AppInfo::launch_default_for_uri(
                    file.uri().as_str(),
                    None::<&gio::AppLaunchContext>,
                ) {
                    let toast = adw::Toast::new(&gettext("Failed to open output file"));
                    root.add_toast(toast);
                    tracing::error!("Failed to open output file: {:?}", e);
                }
            }
            MergePageMsg::AddFiles(files) => {
                self.is_adding_files = true;
                self.check_loading_state(&sender);

                let sender_clone = sender.clone();
                relm4::spawn_blocking(move || {
                    let prepared: Vec<PreparedFile> = files
                        .into_iter()
                        .map(|file| {
                            let title = file_title(&file);
                            let size_str = file_size_string(&file);
                            PreparedFile {
                                item_type: MergeItemType::File(file),
                                title,
                                size_str,
                                rotation: 0,
                                password: None,
                                thumbnail: None,
                                original_dimensions: None,
                            }
                        })
                        .collect();
                    sender_clone.input(MergePageMsg::FilesReady(prepared));
                });
            }
            MergePageMsg::InsertBlankPageAfter {
                index,
                width,
                height,
                rotation,
            } => {
                let current_index = index.current_index();
                let prepared = PreparedFile {
                    item_type: MergeItemType::BlankPage { width, height },
                    title: gettext("Blank Page"),
                    size_str: "-".to_string(),
                    rotation,
                    password: None,
                    thumbnail: None,
                    original_dimensions: Some((width, height)),
                };
                self.files.guard().insert(current_index + 1, prepared);
                self.update_bounds();
                let _ = sender.output(MergePageOutput::FileCountChanged(self.files.len()));
                self.check_loading_state(&sender);
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
                self.is_adding_files = false;
                self.check_loading_state(&sender);
            }
            MergePageMsg::ClearFiles => {
                self.password_queue.clear();
                {
                    let mut files_guard = self.files.guard();
                    files_guard.clear();
                }

                let _ = sender.output(MergePageOutput::FileCountChanged(0));
                let _ = sender.output(MergePageOutput::ClearFiles);
                self.check_loading_state(&sender);
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
            MergePageMsg::DuplicateFile(index) => {
                let current_index = index.current_index();

                let prepared = self
                    .files
                    .guard()
                    .get(current_index)
                    .map(|row| row.to_prepared_file());

                if let Some(prepared) = prepared {
                    self.files.guard().insert(current_index + 1, prepared);
                    self.update_bounds();
                    let _ = sender.output(MergePageOutput::FileCountChanged(self.files.len()));
                    self.check_loading_state(&sender);
                }
            }
            MergePageMsg::DeleteFile { index, show_toast } => {
                let current_index = index.current_index();

                let removed_file = self
                    .files
                    .guard()
                    .get(current_index)
                    .map(|row| row.to_prepared_file());

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
                self.check_loading_state(&sender);

                if show_toast && let Some(prepared) = removed_file {
                    let toast_msg =
                        gettext("Removed \"{filename}\"").replace("{filename}", &prepared.title);
                    let toast = adw::Toast::new(&toast_msg);
                    toast.set_button_label(Some(&gettext("Undo")));
                    toast.set_priority(adw::ToastPriority::High);

                    let sender_clone = sender.clone();
                    toast.connect_button_clicked(move |_| {
                        sender_clone.input(MergePageMsg::UndoDeleteFile {
                            index: current_index,
                            prepared: prepared.clone(),
                        });
                    });

                    root.add_toast(toast);
                }
            }
            MergePageMsg::UndoDeleteFile { index, prepared } => {
                let mut files_guard = self.files.guard();
                let insert_index = index.min(files_guard.len());
                if insert_index == files_guard.len() {
                    files_guard.push_back(prepared);
                } else {
                    files_guard.insert(insert_index, prepared);
                }
                let len = files_guard.len();
                drop(files_guard);

                let _ = sender.output(MergePageOutput::FileCountChanged(len));
                self.update_bounds();
                self.check_loading_state(&sender);
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
                self.check_loading_state(&sender);
            }
            MergePageMsg::PreviewComplete => {
                self.check_loading_state(&sender);
            }
            MergePageMsg::PasswordDialogOutput(output) => match output {
                PasswordDialogOutput::Unlock { index, password } => {
                    if let Some(idx) = index {
                        self.files.send(
                            idx.current_index(),
                            MergeFileRowMsg::RetryWithPassword(password),
                        );
                    }
                }
                PasswordDialogOutput::Cancelled(index) => {
                    if let Some(idx) = index {
                        sender.input(MergePageMsg::DeleteFile {
                            index: idx,
                            show_toast: false,
                        });
                    }
                }
            },
        }
    }
}

impl MergePage {
    fn check_loading_state(&mut self, sender: &ComponentSender<Self>) {
        let is_loading = self.is_adding_files || self.is_merging || {
            let files_guard = self.files.guard();
            files_guard.iter().any(|row| {
                matches!(
                    row.preview_status,
                    PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
                )
            })
        };

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(MergePageOutput::Loading(is_loading));
        }
    }
    fn process_password_queue(&mut self, root: &<Self as Component>::Root) {
        if let Some(req) = self.password_queue.front()
            && let Some(window) = root.root().and_downcast::<gtk::Window>()
        {
            self.password_dialog.emit(PasswordDialogMsg::Show {
                index: Some(req.index.clone()),
                filename: req.filename.clone(),
                is_error: req.is_error,
                parent_window: window,
            });
        }
    }

    fn update_bounds(&mut self) {
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

relm4::new_action_group!(pub(super) RowActionGroup, "row");
relm4::new_stateless_action!(MoveUpAction, RowActionGroup, "move-up");
relm4::new_stateless_action!(MoveDownAction, RowActionGroup, "move-down");
relm4::new_stateless_action!(DuplicateAction, RowActionGroup, "duplicate");
relm4::new_stateless_action!(InsertBlankAction, RowActionGroup, "insert-blank");

struct MergeFileRow {
    item_type: MergeItemType,
    title: String,
    size_str: String,
    rotation: u16,
    is_first: bool,
    is_last: bool,
    thumbnail: Option<gdk::MemoryTexture>,
    original_dimensions: Option<(f64, f64)>,
    password: Option<SecretString>,
    index: DynamicIndex,
    preview_status: PreviewStatus,
    action_group: gio::SimpleActionGroup,
    move_up_action: gio::SimpleAction,
    move_down_action: gio::SimpleAction,
    insert_blank_action: gio::SimpleAction,
}

#[derive(Debug)]
enum MergeFileRowMsg {
    RotateClockwise,
    UpdateBounds { is_first: bool, is_last: bool },
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    RetryWithPassword(SecretString),
    RequestInsertBlank,
}

#[derive(Debug)]
enum MergeFileRowOutput {
    MoveUp(DynamicIndex),
    MoveDown(DynamicIndex),
    Duplicate(DynamicIndex),
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
    PreviewComplete,
    InsertBlankPageAfter {
        index: DynamicIndex,
        width: f64,
        height: f64,
        rotation: u16,
    },
}

impl MergeFileRow {
    fn to_prepared_file(&self) -> PreparedFile {
        PreparedFile {
            item_type: self.item_type.clone(),
            title: self.title.clone(),
            size_str: self.size_str.clone(),
            rotation: self.rotation,
            password: self.password.clone(),
            thumbnail: self.thumbnail.clone(),
            original_dimensions: self.original_dimensions,
        }
    }
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

            add_prefix = &gtk::Image {
                set_icon_name: Some("list-drag-handle-symbolic"),
                add_css_class: "dim-label",
                set_valign: gtk::Align::Center,
                set_cursor_from_name: Some("grab"),
            },

            add_controller = gtk::DragSource {
                set_actions: gdk::DragAction::MOVE,

                connect_prepare[index] => move |drag_source, _x, _y| {
                    if let Some(device) = drag_source.current_event_device()
                        && device.source() == gdk::InputSource::Touchscreen {
                            return None;
                        }
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

            add_controller = gtk::GestureClick::new() {
                set_button: 3,
                connect_pressed[menu_button] => move |gesture, _, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    menu_button.popup();
                }
            },

            add_controller = gtk::GestureLongPress::new() {
                connect_pressed[menu_button] => move |gesture, _, _| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    menu_button.popup();
                }
            },

            add_suffix = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,

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

                #[name(menu_button)]
                append = &gtk::MenuButton {
                    set_icon_name: "view-more-symbolic",
                    add_css_class: "flat",
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some(&gettext("More Options")),

                    insert_action_group: ("row", Some(&self.action_group)),

                    set_menu_model: Some(&{
                        relm4::menu! {
                            main_menu: {
                                section! {
                                    &gettext("Move _Up") => MoveUpAction,
                                    &gettext("_Move Down") => MoveDownAction,
                                    &gettext("_Duplicate") => DuplicateAction,
                                    &gettext("_Insert Blank Page After") => InsertBlankAction,
                                }
                            }
                        }
                        main_menu
                    }),
                }
            }
        }
    }

    fn init_model(prepared: Self::Init, index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let item_type = prepared.item_type.clone();
        let sender_clone = sender.clone();

        let rotation = prepared.rotation;
        let password = prepared.password.clone();
        let thumbnail = prepared.thumbnail;
        let original_dimensions = prepared.original_dimensions;

        if thumbnail.is_none() {
            request_thumbnail(item_type.clone(), rotation, password.clone(), sender_clone);
        }

        let action_group = gio::SimpleActionGroup::new();
        let move_up_action = gio::SimpleAction::new("move-up", None);
        let move_down_action = gio::SimpleAction::new("move-down", None);
        let duplicate_action = gio::SimpleAction::new("duplicate", None);
        let insert_blank_action = gio::SimpleAction::new("insert-blank", None);

        let sender_up = sender.clone();
        let index_up = index.clone();
        move_up_action.connect_activate(move |_, _| {
            let _ = sender_up.output(MergeFileRowOutput::MoveUp(index_up.clone()));
        });

        let sender_down = sender.clone();
        let index_down = index.clone();
        move_down_action.connect_activate(move |_, _| {
            let _ = sender_down.output(MergeFileRowOutput::MoveDown(index_down.clone()));
        });

        let sender_dup = sender.clone();
        let index_dup = index.clone();
        duplicate_action.connect_activate(move |_, _| {
            let _ = sender_dup.output(MergeFileRowOutput::Duplicate(index_dup.clone()));
        });

        let sender_insert = sender.clone();
        insert_blank_action.connect_activate(move |_, _| {
            sender_insert.input(MergeFileRowMsg::RequestInsertBlank);
        });

        let is_first = index.current_index() == 0;
        let is_last = false;

        move_up_action.set_enabled(!is_first);
        move_down_action.set_enabled(!is_last);
        insert_blank_action.set_enabled(original_dimensions.is_some());

        action_group.add_action(&move_up_action);
        action_group.add_action(&move_down_action);
        action_group.add_action(&duplicate_action);
        action_group.add_action(&insert_blank_action);

        let preview_status = if thumbnail.is_some() {
            PreviewStatus::Ready
        } else {
            PreviewStatus::InitialPending
        };

        Self {
            item_type: prepared.item_type,
            title: prepared.title,
            size_str: prepared.size_str,
            rotation: prepared.rotation,
            is_first,
            is_last,
            thumbnail,
            original_dimensions,
            password,
            index: index.clone(),
            preview_status,
            action_group,
            move_up_action,
            move_down_action,
            insert_blank_action,
        }
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            MergeFileRowMsg::RotateClockwise => {
                self.rotation = (self.rotation + 90) % 360;
                self.preview_status = PreviewStatus::Reloading;

                let item_clone = self.item_type.clone();
                let rotation = self.rotation;
                let sender_clone = sender.clone();
                let password = self.password.clone();

                request_thumbnail(item_clone, rotation, password, sender_clone);
            }
            MergeFileRowMsg::UpdateBounds { is_first, is_last } => {
                self.is_first = is_first;
                self.is_last = is_last;
                self.move_up_action.set_enabled(!is_first);
                self.move_down_action.set_enabled(!is_last);
            }
            MergeFileRowMsg::ThumbnailReady(result) => {
                match result {
                    Ok(thumb_res) => {
                        self.thumbnail = thumb_res.texture;
                        if let Some(dim) = thumb_res.original_dimensions {
                            self.original_dimensions = Some(dim);
                            self.insert_blank_action.set_enabled(true);
                        }
                    }
                    Err(PreviewError::Encrypted) => {
                        self.preview_status = PreviewStatus::PasswordRequired;
                        let is_error = self.password.is_some();
                        let _ = sender.output(MergeFileRowOutput::PasswordRequired {
                            index: self.index.clone(),
                            filename: self.title.clone(),
                            is_error,
                        });
                        return;
                    }
                    Err(_) => {}
                }

                self.preview_status = PreviewStatus::Ready;
                if self.password.is_some() {
                    let _ = sender.output(MergeFileRowOutput::PasswordSuccess(self.index.clone()));
                } else {
                    let _ = sender.output(MergeFileRowOutput::PreviewComplete);
                }
            }
            MergeFileRowMsg::RetryWithPassword(password) => {
                self.password = Some(password.clone());

                let item_clone = self.item_type.clone();
                let rotation = self.rotation;
                let sender_clone = sender.clone();

                request_thumbnail(item_clone, rotation, Some(password), sender_clone);
            }
            MergeFileRowMsg::RequestInsertBlank => {
                if let Some((w, h)) = self.original_dimensions {
                    let _ = sender.output(MergeFileRowOutput::InsertBlankPageAfter {
                        index: self.index.clone(),
                        width: w,
                        height: h,
                        rotation: self.rotation,
                    });
                }
            }
        }
    }
}

fn request_thumbnail(
    item_type: MergeItemType,
    rotation: u16,
    password: Option<SecretString>,
    sender: FactorySender<MergeFileRow>,
) {
    if let Err(e) = crate::pdf::preview::thread_pool().push(move || {
        let result = match item_type {
            MergeItemType::File(file) => crate::pdf::preview::generate_thumbnail(
                &file,
                rotation as i32,
                password.as_deref(),
                150.0,
            ),
            MergeItemType::BlankPage { width, height } => {
                crate::pdf::preview::generate_blank_thumbnail(width, height, rotation as i32, 150.0)
            }
        };
        sender.input(MergeFileRowMsg::ThumbnailReady(result));
    }) {
        tracing::error!("Failed to enqueue thumbnail task: {}", e);
    }
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
