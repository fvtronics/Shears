use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio};

use crate::modals::password::{PasswordDialog, PasswordDialogMsg, PasswordDialogOutput};
use crate::pdf::preview::PreviewError;
use crate::pdf::{DivideAfter, PdfError, SplitOptions, split_file};
use crate::tools::page::ToolPage;
use crate::tools::{Tool, file_stem, open_pdf_dialog, select_folder_dialog};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SplitToolState {
    Empty,
    LoadingNewFile,
    Ready,
    Processing,
}

pub struct SplitTool {
    state: SplitToolState,
    _empty_page: Controller<ToolPage>,
    split_page: Controller<SplitPage>,
}

#[derive(Debug)]
pub enum SplitToolMsg {
    AddFiles(Vec<gio::File>),
    UpdateFileActive(Option<String>),
    Loading(bool),
}

#[derive(Debug)]
pub enum SplitToolOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for SplitTool {
    type Init = ();
    type Input = SplitToolMsg;
    type Output = SplitToolOutput;

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.split_page.widget(), Some("split")),

            #[watch]
            set_visible_child_name: if matches!(model.state, SplitToolState::Ready | SplitToolState::Processing) { "split" } else { "empty" },
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

        let split_page = SplitPage::builder()
            .launch(())
            .forward(sender.input_sender(), |msg| match msg {
                SplitPageOutput::FileActive(file_stem) => SplitToolMsg::UpdateFileActive(file_stem),
                SplitPageOutput::Loading(is_loading) => SplitToolMsg::Loading(is_loading),
            });

        let model = Self {
            state: SplitToolState::Empty,
            _empty_page: empty_page,
            split_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            SplitToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.split_page.emit(SplitPageMsg::AddFile(file));
                }
            }
            SplitToolMsg::UpdateFileActive(file_stem) => {
                if file_stem.is_none() {
                    self.state = SplitToolState::Empty;
                }
                let _ = sender.output(SplitToolOutput::FileActive(file_stem));
            }
            SplitToolMsg::Loading(is_loading) => {
                if is_loading {
                    if self.state == SplitToolState::Empty {
                        self.state = SplitToolState::LoadingNewFile;
                    } else if self.state == SplitToolState::Ready {
                        self.state = SplitToolState::Processing;
                    }
                } else {
                    if self.state == SplitToolState::LoadingNewFile
                        || self.state == SplitToolState::Processing
                    {
                        self.state = SplitToolState::Ready;
                    }
                }
                self._empty_page.emit(is_loading);
                let _ = sender.output(SplitToolOutput::Loading(is_loading));
            }
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PreviewStatus {
    InitialPending,
    Ready,
    PasswordRequired,
}

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum DivideMode {
    #[default]
    EachPage,
    EvenPages,
    OddPages,
    EveryNPages,
    SpecificPages,
}

impl From<u32> for DivideMode {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::EvenPages,
            2 => Self::OddPages,
            3 => Self::EveryNPages,
            4 => Self::SpecificPages,
            _ => Self::EachPage,
        }
    }
}

struct SplitPage {
    file: Option<gio::File>,
    password: Option<String>,
    prefix: String,
    prefix_changed: bool,
    divide_mode: DivideMode,
    every_n: u32,
    specific_pages: String,
    specific_pages_error: Option<String>,
    page_count: u32,
    is_splitting: bool,
    is_loading: bool,
    modern_pdf_format: bool,
    remove_metadata: bool,
    thumbnail: Option<gdk::MemoryTexture>,
    password_dialog: Controller<PasswordDialog>,
    preview_status: PreviewStatus,
}

#[derive(Debug)]
enum SplitPageMsg {
    AddFile(gio::File),
    SetDivideMode(DivideMode),
    SetEveryN(u32),
    SetSpecificPages(String),
    SetPrefix(String),
    SplitTo(gio::File),
    SetModernPdfFormat(bool),
    SetRemoveMetadata(bool),
    SplitComplete(Result<std::path::PathBuf, PdfError>),
    OpenOutput(std::path::PathBuf),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    PasswordDialogOutput(PasswordDialogOutput),
}

#[derive(Debug)]
pub enum SplitPageOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component]
impl Component for SplitPage {
    type Init = ();
    type Input = SplitPageMsg;
    type Output = SplitPageOutput;
    type CommandOutput = ();

    view! {
        #[root]
        adw::ToastOverlay {
            #[name(breakpoint_bin)]
            adw::BreakpointBin {
                set_width_request: 260,
                set_height_request: 200,

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
                            set_label: &Tool::Split.action_label(),
                            set_tooltip_text: Some(&gettext("Select PDF File")),

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

                        gtk::MenuButton {
                            set_icon_name: "view-more-symbolic",
                            add_css_class: "flat",
                            set_tooltip_text: Some(&gettext("Advanced Options")),

                            #[wrap(Some)]
                            set_popover = &gtk::Popover {
                                add_css_class: "menu",
                                adw::PreferencesGroup {
                                    add = &adw::SwitchRow {
                                        set_title: &gettext("_Modern PDF format"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Save with PDF 1.5 object streams"),
                                        set_active: model.modern_pdf_format,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(SplitPageMsg::SetModernPdfFormat(row.is_active()));
                                        }
                                    },

                                    add = &adw::SwitchRow {
                                        set_title: &gettext("_Remove metadata"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Remove existing metadata before saving"),
                                        set_active: model.remove_metadata,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(SplitPageMsg::SetRemoveMetadata(row.is_active()));
                                        }
                                    },
                                }
                            }
                        }
                    },

                    #[name(split_box)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 24,
                        set_margin_all: 24,
                        #[watch]
                        set_sensitive: !model.is_loading,

                        #[name(picture_clamp)]
                        adw::Clamp {
                            set_maximum_size: 450,
                            set_unit: adw::LengthUnit::Sp,

                            #[wrap(Some)]
                            set_child = &gtk::Picture {
                                set_can_shrink: true,
                                set_content_fit: gtk::ContentFit::Contain,
                                #[watch]
                                set_paintable: model.thumbnail.as_ref(),
                            }
                        },

                        gtk::ScrolledWindow {
                            set_width_request: 256,
                            set_vexpand: true,
                            set_hexpand: true,
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
                                            gettext("Every N pages").as_str(),
                                            gettext("Specific pages").as_str(),
                                        ])),
                                        connect_selected_notify[sender] => move |row| {
                                            sender.input(SplitPageMsg::SetDivideMode(DivideMode::from(row.selected())));
                                        }
                                    },

                                    adw::SpinRow {
                                        set_title: &gettext("Pages"),
                                        set_numeric: true,
                                        #[wrap(Some)]
                                        set_adjustment = &gtk::Adjustment {
                                            set_lower: 1.0,
                                            #[watch]
                                            set_upper: model.page_count.max(1) as f64,
                                            set_value: 1.0,
                                            set_step_increment: 1.0,
                                            set_page_increment: 10.0,
                                        },
                                        #[watch]
                                        set_visible: matches!(model.divide_mode, DivideMode::EveryNPages),
                                        connect_value_notify[sender] => move |row| {
                                            sender.input(SplitPageMsg::SetEveryN(row.value() as u32));
                                        }
                                    },

                                    adw::EntryRow {
                                        set_title: &gettext("Pages"),
                                        #[watch]
                                        set_visible: matches!(model.divide_mode, DivideMode::SpecificPages),
                                        set_text: &model.specific_pages,
                                        set_show_apply_button: false,
                                        #[watch]
                                        set_class_active: ("error", model.specific_pages_error.is_some()),
                                        connect_changed[sender] => move |entry| {
                                            sender.input(SplitPageMsg::SetSpecificPages(entry.text().to_string()));
                                        },
                                        add_suffix = &gtk::Image {
                                            set_icon_name: Some("dialog-error-symbolic"),
                                            #[watch]
                                            set_tooltip_text: model.specific_pages_error.as_deref(),
                                            #[watch]
                                            set_visible: model.specific_pages_error.is_some(),
                                            add_css_class: "error",
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
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let password_dialog = PasswordDialog::builder()
            .launch(())
            .forward(sender.input_sender(), SplitPageMsg::PasswordDialogOutput);

        let model = Self {
            file: None,
            password: None,
            prefix: gettext("output_file"),
            prefix_changed: true,
            divide_mode: DivideMode::default(),
            every_n: 1,
            specific_pages: String::new(),
            specific_pages_error: None,
            page_count: 0,
            is_splitting: false,
            is_loading: false,
            modern_pdf_format: false,
            remove_metadata: false,
            thumbnail: None,
            password_dialog,
            preview_status: PreviewStatus::Ready,
        };
        let widgets = view_output!();

        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            600.0,
            adw::LengthUnit::Sp,
        );
        let bp = adw::Breakpoint::new(condition);
        bp.add_setters(&[(
            &widgets.split_box,
            "orientation",
            gtk::Orientation::Vertical,
        )]);
        bp.add_setters(&[(&widgets.picture_clamp, "vexpand", true)]);
        widgets.breakpoint_bin.add_breakpoint(bp);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        self.prefix_changed = false;
        match message {
            SplitPageMsg::AddFile(file) => {
                self.prefix = file_stem(&file);
                self.prefix_changed = true;
                self.password = None;
                self.preview_status = PreviewStatus::InitialPending;
                self.file = Some(file.clone());

                self.check_loading_state(&sender);
                let _ = sender.output(SplitPageOutput::FileActive(Some(self.prefix.clone())));

                self.request_thumbnail(None, &sender);
            }
            SplitPageMsg::SetDivideMode(divide_mode) => {
                self.divide_mode = divide_mode;
            }
            SplitPageMsg::SetEveryN(every_n) => {
                self.every_n = every_n;
            }
            SplitPageMsg::SetSpecificPages(pages) => {
                self.specific_pages = pages;
                self.specific_pages_error = None;
            }
            SplitPageMsg::SetPrefix(prefix) => {
                self.prefix = prefix;
            }
            SplitPageMsg::SplitTo(output_folder) => {
                if let (Some(file_path), Some(output_path)) = (
                    self.file.as_ref().and_then(|f| f.path()),
                    output_folder.path(),
                ) {
                    let divide_after = match self.divide_mode {
                        DivideMode::EachPage => DivideAfter::EachPage,
                        DivideMode::EvenPages => DivideAfter::EvenPages,
                        DivideMode::OddPages => DivideAfter::OddPages,
                        DivideMode::EveryNPages => {
                            DivideAfter::EveryNPages(self.every_n.min(self.page_count.max(1)))
                        }
                        DivideMode::SpecificPages => {
                            match super::validate_specific_pages(
                                &self.specific_pages,
                                self.page_count,
                            ) {
                                Ok(cleaned) => DivideAfter::SpecificPages(cleaned),
                                Err(err) => {
                                    self.specific_pages_error = Some(err);
                                    return;
                                }
                            }
                        }
                    };

                    self.is_splitting = true;
                    self.check_loading_state(&sender);
                    let options = SplitOptions {
                        divide_after,
                        prefix: self.prefix.clone(),
                        password: self.password.clone(),
                        modern_format: self.modern_pdf_format,
                        remove_metadata: self.remove_metadata,
                    };
                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = split_file(&(file_path, 0), output_path.clone(), &options);
                        let msg_result = result.map(|_| output_path);
                        sender.input(SplitPageMsg::SplitComplete(msg_result));
                    });
                }
            }
            SplitPageMsg::SetModernPdfFormat(active) => {
                self.modern_pdf_format = active;
            }
            SplitPageMsg::SetRemoveMetadata(active) => {
                self.remove_metadata = active;
            }
            SplitPageMsg::SplitComplete(result) => {
                self.is_splitting = false;
                self.check_loading_state(&sender);
                match result {
                    Ok(path) => {
                        tracing::info!("Split PDF complete");
                        let toast = adw::Toast::new(&gettext("Split PDFs saved"));
                        toast.set_button_label(Some(&gettext("Open Folder")));
                        let sender_clone = sender.clone();
                        toast.connect_button_clicked(move |_| {
                            sender_clone.input(SplitPageMsg::OpenOutput(path.clone()));
                        });
                        root.add_toast(toast);
                    }
                    Err(err) => {
                        tracing::error!("Failed to split PDF: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to split PDF")));
                    }
                }
            }
            SplitPageMsg::OpenOutput(path) => {
                let file = gio::File::for_path(&path);
                if let Err(e) = gio::AppInfo::launch_default_for_uri(
                    file.uri().as_str(),
                    None::<&gio::AppLaunchContext>,
                ) {
                    let toast = adw::Toast::new(&gettext("Failed to open output folder"));
                    root.add_toast(toast);
                    tracing::error!("Failed to open output folder: {:?}", e);
                }
            }
            SplitPageMsg::ThumbnailReady(result) => {
                match result {
                    Ok(thumb_res) => {
                        self.thumbnail = thumb_res.texture;
                        self.page_count = thumb_res.page_count as u32;
                        self.preview_status = PreviewStatus::Ready;
                    }
                    Err(PreviewError::Encrypted) => {
                        self.preview_status = PreviewStatus::PasswordRequired;
                        let is_error = self.password.is_some();
                        let filename = format!("{}.pdf", self.prefix);
                        if let Some(window) = root.root().and_downcast::<gtk::Window>() {
                            self.password_dialog.emit(PasswordDialogMsg::Show {
                                index: None,
                                filename,
                                is_error,
                                parent_window: window,
                            });
                        }
                    }
                    Err(err) => {
                        tracing::warn!("Failed to generate thumbnail for split page: {:?}", err);
                        self.thumbnail = None;
                        self.preview_status = PreviewStatus::Ready;
                    }
                }
                self.check_loading_state(&sender);
            }
            SplitPageMsg::PasswordDialogOutput(output) => match output {
                PasswordDialogOutput::Unlock { password, .. } => {
                    self.password = Some(password.clone());
                    self.request_thumbnail(Some(password), &sender);
                }
                PasswordDialogOutput::Cancelled(_) => {
                    self.clear_file(&sender);
                }
            },
        }
    }
}

impl SplitPage {
    fn request_thumbnail(&self, password: Option<String>, sender: &ComponentSender<Self>) {
        if let Some(file) = &self.file {
            let sender_clone = sender.clone();
            let file_clone = file.clone();

            if let Err(e) = crate::pdf::preview::thread_pool().push(move || {
                let result = crate::pdf::preview::generate_thumbnail(
                    &file_clone,
                    0,
                    password.as_deref(),
                    800.0,
                );
                sender_clone.input(SplitPageMsg::ThumbnailReady(result));
            }) {
                tracing::error!("Failed to enqueue thumbnail task: {}", e);
            }
        }
    }

    fn clear_file(&mut self, sender: &ComponentSender<Self>) {
        self.file = None;
        self.thumbnail = None;
        self.password = None;
        self.preview_status = PreviewStatus::Ready;
        self.check_loading_state(sender);
        let _ = sender.output(SplitPageOutput::FileActive(None));
    }

    fn check_loading_state(&mut self, sender: &ComponentSender<Self>) {
        let is_loading = self.is_splitting
            || matches!(
                self.preview_status,
                PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
            );

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(SplitPageOutput::Loading(is_loading));
        }
    }
}
