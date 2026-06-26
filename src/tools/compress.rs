use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio};

use crate::modals::password::{PasswordDialog, PasswordDialogMsg, PasswordDialogOutput};
use crate::pdf::preview::PreviewError;
use crate::pdf::{CompressOptions, PdfError, compress_file};
use crate::tools::page::ToolPage;
use crate::tools::{PreviewStatus, Tool, ToolState, file_stem, open_pdf_dialog, save_pdf_dialog};

pub struct CompressTool {
    state: ToolState,
    _empty_page: Controller<ToolPage>,
    compress_page: Controller<CompressPage>,
}

#[derive(Debug)]
pub enum CompressToolMsg {
    AddFiles(Vec<gio::File>),
    UpdateFileActive(Option<String>),
    Loading(bool),
}

#[derive(Debug)]
pub enum CompressToolOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for CompressTool {
    type Init = ();
    type Input = CompressToolMsg;
    type Output = CompressToolOutput;

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.compress_page.widget(), Some("compress")),

            #[watch]
            set_visible_child_name: if matches!(model.state, ToolState::Ready | ToolState::Processing) { "compress" } else { "empty" },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let empty_page = ToolPage::builder()
            .launch(Tool::Compress)
            .forward(sender.input_sender(), CompressToolMsg::AddFiles);

        let compress_page =
            CompressPage::builder()
                .launch(())
                .forward(sender.input_sender(), |msg| match msg {
                    CompressPageOutput::FileActive(file_stem) => {
                        CompressToolMsg::UpdateFileActive(file_stem)
                    }
                    CompressPageOutput::Loading(is_loading) => CompressToolMsg::Loading(is_loading),
                });

        let model = Self {
            state: ToolState::Empty,
            _empty_page: empty_page,
            compress_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            CompressToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.compress_page.emit(CompressPageMsg::AddFile(file));
                }
            }
            CompressToolMsg::UpdateFileActive(file_stem) => {
                if file_stem.is_none() {
                    self.state = ToolState::Empty;
                }
                let _ = sender.output(CompressToolOutput::FileActive(file_stem));
            }
            CompressToolMsg::Loading(is_loading) => {
                if is_loading {
                    if self.state == ToolState::Empty {
                        self.state = ToolState::LoadingNewFile;
                    } else if self.state == ToolState::Ready {
                        self.state = ToolState::Processing;
                    }
                } else {
                    if self.state == ToolState::LoadingNewFile
                        || self.state == ToolState::Processing
                    {
                        self.state = ToolState::Ready;
                    }
                }
                self._empty_page.emit(is_loading);
                let _ = sender.output(CompressToolOutput::Loading(is_loading));
            }
        }
    }
}

struct CompressPage {
    file: Option<gio::File>,
    password: Option<String>,

    remove_unused_data: bool,
    remove_empty_streams: bool,
    modern_pdf_format: bool,
    remove_metadata: bool,

    is_saving: bool,
    is_loading: bool,
    thumbnail: Option<gdk::MemoryTexture>,
    password_dialog: Controller<PasswordDialog>,
    preview_status: PreviewStatus,
}

#[derive(Debug)]
enum CompressPageMsg {
    AddFile(gio::File),
    SetRemoveUnusedData(bool),
    SetRemoveEmptyStreams(bool),
    SetModernPdfFormat(bool),
    SetRemoveMetadata(bool),
    SaveTo(gio::File),
    SaveComplete(Result<std::path::PathBuf, PdfError>),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    PasswordDialogOutput(PasswordDialogOutput),
    OpenOutput(std::path::PathBuf),
}

#[derive(Debug)]
pub enum CompressPageOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component]
impl Component for CompressPage {
    type Init = ();
    type Input = CompressPageMsg;
    type Output = CompressPageOutput;
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
                            set_label: &Tool::Compress.action_label(),
                            set_tooltip_text: Some(&gettext("Select PDF File")),

                            connect_clicked[sender] => move |button| {
                                let sender_clone = sender.clone();
                                open_pdf_dialog(button, Tool::Compress, move |mut files| {
                                    if let Some(file) = files.pop() {
                                        sender_clone.input(CompressPageMsg::AddFile(file));
                                    }
                                });
                            },
                        },

                        gtk::Button {
                            set_label: &gettext("Compress"),
                            set_tooltip_text: Some(&gettext("Compress PDF")),
                            add_css_class: "suggested-action",
                            #[watch]
                            set_sensitive: model.file.is_some() && !model.is_saving,

                            connect_clicked[sender] => move |button| {
                                let sender_clone = sender.clone();
                                save_pdf_dialog(button, Tool::Compress, &gettext("Save PDF"), move |file| {
                                    sender_clone.input(CompressPageMsg::SaveTo(file));
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
                                            sender.input(CompressPageMsg::SetModernPdfFormat(row.is_active()));
                                        }
                                    },

                                    add = &adw::SwitchRow {
                                        set_title: &gettext("_Remove metadata"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Remove existing metadata before saving"),
                                        set_active: model.remove_metadata,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(CompressPageMsg::SetRemoveMetadata(row.is_active()));
                                        }
                                    },
                                }
                            }
                        }
                    },

                    #[name(compress_box)]
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
                                    add = &adw::SwitchRow {
                                        set_title: &gettext("Remove Unused Data"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Discard unreferenced PDF objects"),
                                        set_active: model.remove_unused_data,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(CompressPageMsg::SetRemoveUnusedData(row.is_active()));
                                        }
                                    },

                                    add = &adw::SwitchRow {
                                        set_title: &gettext("Remove Empty Streams"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Discard empty PDF data streams"),
                                        set_active: model.remove_empty_streams,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(CompressPageMsg::SetRemoveEmptyStreams(row.is_active()));
                                        }
                                    },
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
            .forward(sender.input_sender(), CompressPageMsg::PasswordDialogOutput);

        let model = Self {
            file: None,
            password: None,
            remove_unused_data: true,
            remove_empty_streams: true,
            modern_pdf_format: false,
            remove_metadata: false,
            is_saving: false,
            is_loading: false,
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
            &widgets.compress_box,
            "orientation",
            gtk::Orientation::Vertical,
        )]);
        bp.add_setters(&[(&widgets.picture_clamp, "vexpand", true)]);
        widgets.breakpoint_bin.add_breakpoint(bp);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            CompressPageMsg::AddFile(file) => {
                self.password = None;
                self.preview_status = PreviewStatus::InitialPending;

                let stem = file_stem(&file);
                self.file = Some(file.clone());

                self.check_loading_state(&sender);
                let _ = sender.output(CompressPageOutput::FileActive(Some(stem)));

                self.request_thumbnail(None, &sender);
            }
            CompressPageMsg::SetRemoveUnusedData(val) => {
                self.remove_unused_data = val;
            }
            CompressPageMsg::SetRemoveEmptyStreams(val) => {
                self.remove_empty_streams = val;
            }
            CompressPageMsg::SetModernPdfFormat(val) => {
                self.modern_pdf_format = val;
            }
            CompressPageMsg::SetRemoveMetadata(val) => {
                self.remove_metadata = val;
            }
            CompressPageMsg::SaveTo(output_file) => {
                if let (Some(file_path), Some(output_path)) = (
                    self.file.as_ref().and_then(|f| f.path()),
                    output_file.path(),
                ) {
                    self.is_saving = true;
                    self.check_loading_state(&sender);

                    let options = CompressOptions {
                        remove_unused_data: self.remove_unused_data,
                        remove_empty_streams: self.remove_empty_streams,
                        modern_pdf_format: self.modern_pdf_format,
                        remove_metadata: self.remove_metadata,
                        password: self.password.clone(),
                    };

                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = compress_file(&(file_path, 0), output_path.clone(), &options);
                        match result {
                            Ok(_) => sender.input(CompressPageMsg::SaveComplete(Ok(output_path))),
                            Err(e) => sender.input(CompressPageMsg::SaveComplete(Err(e))),
                        }
                    });
                }
            }
            CompressPageMsg::SaveComplete(result) => {
                self.is_saving = false;
                self.check_loading_state(&sender);
                match result {
                    Ok(path) => {
                        tracing::info!("Compress complete");
                        let toast = adw::Toast::new(&gettext("PDF compressed successfully"));
                        toast.set_button_label(Some(&gettext("Open File")));
                        let sender_clone = sender.clone();
                        toast.connect_button_clicked(move |_| {
                            sender_clone.input(CompressPageMsg::OpenOutput(path.clone()));
                        });
                        root.add_toast(toast);
                    }
                    Err(err) => {
                        tracing::error!("Failed to compress PDF: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to compress PDF")));
                    }
                }
            }
            CompressPageMsg::OpenOutput(path) => {
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
            CompressPageMsg::ThumbnailReady(result) => {
                match result {
                    Ok(thumb_res) => {
                        self.thumbnail = thumb_res.texture;
                        self.preview_status = PreviewStatus::Ready;
                    }
                    Err(PreviewError::Encrypted) => {
                        self.preview_status = PreviewStatus::PasswordRequired;
                        let is_error = self.password.is_some();
                        let filename = self.file.as_ref().map(file_stem).unwrap_or_default();
                        if let Some(window) = root.root().and_downcast::<gtk::Window>() {
                            self.password_dialog.emit(PasswordDialogMsg::Show {
                                index: None,
                                filename: format!("{}.pdf", filename),
                                is_error,
                                parent_window: window,
                            });
                        }
                    }
                    Err(err) => {
                        tracing::warn!("Failed to generate thumbnail for compress page: {:?}", err);
                        self.thumbnail = None;
                        self.preview_status = PreviewStatus::Ready;
                    }
                }
                self.check_loading_state(&sender);
            }
            CompressPageMsg::PasswordDialogOutput(output) => match output {
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

impl CompressPage {
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
                sender_clone.input(CompressPageMsg::ThumbnailReady(result));
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
        let _ = sender.output(CompressPageOutput::FileActive(None));
    }

    fn check_loading_state(&mut self, sender: &ComponentSender<Self>) {
        let is_loading = self.is_saving
            || matches!(
                self.preview_status,
                PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
            );

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(CompressPageOutput::Loading(is_loading));
        }
    }
}
