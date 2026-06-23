use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio};

use crate::modals::password::{PasswordDialog, PasswordDialogMsg, PasswordDialogOutput};
use crate::pdf::preview::PreviewError;
use crate::pdf::{MetadataOptions, PdfMetadata, PdfError, update_metadata, read_metadata};
use crate::tools::page::ToolPage;
use crate::tools::{Tool, file_stem, open_pdf_dialog, save_pdf_dialog};

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MetadataToolState {
    Empty,
    LoadingNewFile,
    Ready,
    Processing,
}

pub struct MetadataTool {
    state: MetadataToolState,
    _empty_page: Controller<ToolPage>,
    metadata_page: Controller<MetadataPage>,
}

#[derive(Debug)]
pub enum MetadataToolMsg {
    AddFiles(Vec<gio::File>),
    UpdateFileActive(Option<String>),
    Loading(bool),
}

#[derive(Debug)]
pub enum MetadataToolOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for MetadataTool {
    type Init = ();
    type Input = MetadataToolMsg;
    type Output = MetadataToolOutput;

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.metadata_page.widget(), Some("metadata")),

            #[watch]
            set_visible_child_name: if matches!(model.state, MetadataToolState::Ready | MetadataToolState::Processing) { "metadata" } else { "empty" },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let empty_page = ToolPage::builder()
            .launch(Tool::Metadata)
            .forward(sender.input_sender(), MetadataToolMsg::AddFiles);

        let metadata_page = MetadataPage::builder()
            .launch(())
            .forward(sender.input_sender(), |msg| match msg {
                MetadataPageOutput::FileActive(file_stem) => MetadataToolMsg::UpdateFileActive(file_stem),
                MetadataPageOutput::Loading(is_loading) => MetadataToolMsg::Loading(is_loading),
            });

        let model = Self {
            state: MetadataToolState::Empty,
            _empty_page: empty_page,
            metadata_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            MetadataToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.metadata_page.emit(MetadataPageMsg::AddFile(file));
                }
            }
            MetadataToolMsg::UpdateFileActive(file_stem) => {
                if file_stem.is_none() {
                    self.state = MetadataToolState::Empty;
                }
                let _ = sender.output(MetadataToolOutput::FileActive(file_stem));
            }
            MetadataToolMsg::Loading(is_loading) => {
                if is_loading {
                    if self.state == MetadataToolState::Empty {
                        self.state = MetadataToolState::LoadingNewFile;
                    } else if self.state == MetadataToolState::Ready {
                        self.state = MetadataToolState::Processing;
                    }
                } else {
                    if self.state == MetadataToolState::LoadingNewFile
                        || self.state == MetadataToolState::Processing
                    {
                        self.state = MetadataToolState::Ready;
                    }
                }
                self._empty_page.emit(is_loading);
                let _ = sender.output(MetadataToolOutput::Loading(is_loading));
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

struct MetadataPage {
    file: Option<gio::File>,
    password: Option<String>,

    title: String,
    author: String,
    subject: String,
    keywords: String,
    creator: String,
    producer: String,

    is_saving: bool,
    is_loading: bool,
    modern_pdf_format: bool,
    remove_metadata: bool,
    sync_entries: bool,
    thumbnail: Option<gdk::MemoryTexture>,
    password_dialog: Controller<PasswordDialog>,
    preview_status: PreviewStatus,
}

#[derive(Debug)]
enum MetadataPageMsg {
    AddFile(gio::File),
    SetTitle(String),
    SetAuthor(String),
    SetSubject(String),
    SetKeywords(String),
    SaveTo(gio::File),
    SaveComplete(Result<std::path::PathBuf, PdfError>),
    SetModernPdfFormat(bool),
    SetRemoveMetadata(bool),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    PasswordDialogOutput(PasswordDialogOutput),
    OpenOutput(std::path::PathBuf),
    MetadataReady(Result<PdfMetadata, PdfError>),
}

#[derive(Debug)]
pub enum MetadataPageOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component]
impl Component for MetadataPage {
    type Init = ();
    type Input = MetadataPageMsg;
    type Output = MetadataPageOutput;
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
                            set_label: &Tool::Metadata.action_label(),
                            set_tooltip_text: Some(&gettext("Select PDF File")),

                            connect_clicked[sender] => move |button| {
                                let sender_clone = sender.clone();
                                open_pdf_dialog(button, Tool::Metadata, move |mut files| {
                                    if let Some(file) = files.pop() {
                                        sender_clone.input(MetadataPageMsg::AddFile(file));
                                    }
                                });
                            },
                        },

                        gtk::Button {
                            set_label: &gettext("Save"),
                            set_tooltip_text: Some(&gettext("Save modified PDF")),
                            add_css_class: "suggested-action",
                            #[watch]
                            set_sensitive: model.file.is_some() && !model.is_saving,

                            connect_clicked[sender] => move |button| {
                                let sender_clone = sender.clone();
                                save_pdf_dialog(button, Tool::Metadata, &gettext("Save PDF"), move |file| {
                                    sender_clone.input(MetadataPageMsg::SaveTo(file));
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
                                            sender.input(MetadataPageMsg::SetModernPdfFormat(row.is_active()));
                                        }
                                    },

                                    add = &adw::SwitchRow {
                                        set_title: &gettext("_Remove metadata"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Remove existing metadata before saving"),
                                        set_active: model.remove_metadata,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(MetadataPageMsg::SetRemoveMetadata(row.is_active()));
                                        }
                                    },
                                }
                            }
                        }
                    },

                    #[name(metadata_box)]
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
                                    adw::EntryRow {
                                        set_title: &gettext("Title"),
                                        #[track = "model.sync_entries"]
                                        set_text: &model.title,
                                        set_show_apply_button: false,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(MetadataPageMsg::SetTitle(entry.text().to_string()));
                                        }
                                    },
                                    adw::EntryRow {
                                        set_title: &gettext("Author"),
                                        #[track = "model.sync_entries"]
                                        set_text: &model.author,
                                        set_show_apply_button: false,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(MetadataPageMsg::SetAuthor(entry.text().to_string()));
                                        }
                                    },
                                    adw::EntryRow {
                                        set_title: &gettext("Subject"),
                                        #[track = "model.sync_entries"]
                                        set_text: &model.subject,
                                        set_show_apply_button: false,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(MetadataPageMsg::SetSubject(entry.text().to_string()));
                                        }
                                    },
                                    adw::EntryRow {
                                        set_title: &gettext("Keywords"),
                                        #[track = "model.sync_entries"]
                                        set_text: &model.keywords,
                                        set_show_apply_button: false,
                                        connect_changed[sender] => move |entry| {
                                            sender.input(MetadataPageMsg::SetKeywords(entry.text().to_string()));
                                        }
                                    },
                                    adw::ActionRow {
                                        set_title: &gettext("Creator"),
                                        #[track = "model.sync_entries"]
                                        set_subtitle: &model.creator,
                                    },
                                    adw::ActionRow {
                                        set_title: &gettext("Producer"),
                                        #[track = "model.sync_entries"]
                                        set_subtitle: &model.producer,

                                        add_suffix = &gtk::Image {
                                            set_icon_name: Some("dialog-warning-symbolic"),
                                            set_tooltip_text: Some(&gettext("Producer will be replaced when saving")),
                                            add_css_class: "warning",
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
            .forward(sender.input_sender(), MetadataPageMsg::PasswordDialogOutput);

        let model = Self {
            file: None,
            password: None,
            title: String::new(),
            author: String::new(),
            subject: String::new(),
            keywords: String::new(),
            creator: gettext("N/A"),
            producer: gettext("N/A"),
            is_saving: false,
            is_loading: false,
            modern_pdf_format: false,
            remove_metadata: false,
            sync_entries: false,
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
            &widgets.metadata_box,
            "orientation",
            gtk::Orientation::Vertical,
        )]);
        bp.add_setters(&[(&widgets.picture_clamp, "vexpand", true)]);
        widgets.breakpoint_bin.add_breakpoint(bp);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        self.sync_entries = false;
        match message {
            MetadataPageMsg::AddFile(file) => {
                self.password = None;
                self.preview_status = PreviewStatus::InitialPending;
                
                self.title.clear();
                self.author.clear();
                self.subject.clear();
                self.keywords.clear();
                self.creator = gettext("N/A");
                self.producer = gettext("N/A");
                self.sync_entries = true;

                let stem = file_stem(&file);
                self.file = Some(file.clone());

                self.check_loading_state(&sender);
                let _ = sender.output(MetadataPageOutput::FileActive(Some(stem)));

                self.request_thumbnail(None, &sender);
                self.request_metadata(None, &sender);
            }
            MetadataPageMsg::SetTitle(val) => { self.title = val; }
            MetadataPageMsg::SetAuthor(val) => { self.author = val; }
            MetadataPageMsg::SetSubject(val) => { self.subject = val; }
            MetadataPageMsg::SetKeywords(val) => { self.keywords = val; }
            MetadataPageMsg::SaveTo(output_file) => {
                if let (Some(file_path), Some(output_path)) = (self.file.as_ref().and_then(|f| f.path()), output_file.path()) {
                    self.is_saving = true;
                    self.check_loading_state(&sender);

                    let options = MetadataOptions {
                        metadata: PdfMetadata {
                            title: self.title.clone(),
                            author: self.author.clone(),
                            subject: self.subject.clone(),
                            keywords: self.keywords.clone(),
                            creator: self.creator.clone(),
                            producer: self.producer.clone(),
                        },
                        modern_pdf_format: self.modern_pdf_format,
                        remove_metadata: self.remove_metadata,
                        password: self.password.clone(),
                    };

                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = update_metadata(&(file_path, 0), output_path.clone(), &options);
                        match result {
                            Ok(_) => sender.input(MetadataPageMsg::SaveComplete(Ok(output_path))),
                            Err(e) => sender.input(MetadataPageMsg::SaveComplete(Err(e))),
                        }
                    });
                }
            }
            MetadataPageMsg::SetModernPdfFormat(active) => {
                self.modern_pdf_format = active;
            }
            MetadataPageMsg::SetRemoveMetadata(active) => {
                self.remove_metadata = active;
            }
            MetadataPageMsg::SaveComplete(result) => {
                self.is_saving = false;
                self.check_loading_state(&sender);
                match result {
                    Ok(path) => {
                        tracing::info!("Save metadata complete");
                        let toast = adw::Toast::new(&gettext("Metadata saved successfully"));
                        toast.set_button_label(Some(&gettext("Open File")));
                        let sender_clone = sender.clone();
                        toast.connect_button_clicked(move |_| {
                            sender_clone.input(MetadataPageMsg::OpenOutput(path.clone()));
                        });
                        root.add_toast(toast);
                    }
                    Err(err) => {
                        tracing::error!("Failed to save metadata: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to save metadata")));
                    }
                }
            }
            MetadataPageMsg::OpenOutput(path) => {
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
            MetadataPageMsg::ThumbnailReady(result) => {
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
                        tracing::warn!("Failed to generate thumbnail for metadata page: {:?}", err);
                        self.thumbnail = None;
                        self.preview_status = PreviewStatus::Ready;
                    }
                }
                self.check_loading_state(&sender);
            }
            MetadataPageMsg::PasswordDialogOutput(output) => match output {
                PasswordDialogOutput::Unlock { password, .. } => {
                    self.password = Some(password.clone());
                    self.request_thumbnail(Some(password.clone()), &sender);
                    self.request_metadata(Some(password), &sender);
                }
                PasswordDialogOutput::Cancelled(_) => {
                    self.clear_file(&sender);
                }
            },
            MetadataPageMsg::MetadataReady(result) => {
                match result {
                    Ok(options) => {
                        self.title = options.title;
                        self.author = options.author;
                        self.subject = options.subject;
                        self.keywords = options.keywords;
                        self.creator = if options.creator.is_empty() { gettext("N/A") } else { options.creator };
                        self.producer = if options.producer.is_empty() { gettext("N/A") } else { options.producer };
                        self.sync_entries = true;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read metadata: {:?}", e);
                        root.add_toast(adw::Toast::new(&gettext("Failed to load document's metadata")));
                    }
                }
            }
        }
    }
}

impl MetadataPage {
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
                sender_clone.input(MetadataPageMsg::ThumbnailReady(result));
            }) {
                tracing::error!("Failed to enqueue thumbnail task: {}", e);
            }
        }
    }

    fn request_metadata(&self, password: Option<String>, sender: &ComponentSender<Self>) {
        if let Some(file) = &self.file {
            let sender_clone = sender.clone();
            if let Some(path) = file.path() {
                relm4::spawn_blocking(move || {
                    let result = read_metadata(&path, password.as_deref());
                    sender_clone.input(MetadataPageMsg::MetadataReady(result));
                });
            }
        }
    }

    fn clear_file(&mut self, sender: &ComponentSender<Self>) {
        self.file = None;
        self.thumbnail = None;
        self.password = None;
        self.preview_status = PreviewStatus::Ready;
        self.check_loading_state(sender);
        let _ = sender.output(MetadataPageOutput::FileActive(None));
    }

    fn check_loading_state(&mut self, sender: &ComponentSender<Self>) {
        let is_loading = self.is_saving
            || matches!(
                self.preview_status,
                PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
            );

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(MetadataPageOutput::Loading(is_loading));
        }
    }
}
