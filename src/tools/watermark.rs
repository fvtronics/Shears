/* tools/watermark.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio};

use crate::modals::password::{PasswordDialog, PasswordDialogMsg, PasswordDialogOutput};
use crate::pdf::preview::PreviewError;
use crate::pdf::{PdfError, WatermarkLayer, WatermarkOptions, WatermarkPages, watermark_file};
use crate::tools::page::ToolPage;
use crate::tools::{PreviewStatus, Tool, ToolState, file_name, open_pdf_dialog, save_pdf_dialog};

pub struct WatermarkTool {
    state: ToolState,
    _empty_page: Controller<ToolPage>,
    watermark_page: Controller<WatermarkPage>,
}

#[derive(Debug)]
pub enum WatermarkToolMsg {
    AddFiles(Vec<gio::File>),
    UpdateFileActive(Option<String>),
    Loading(bool),
}

#[derive(Debug)]
pub enum WatermarkToolOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for WatermarkTool {
    type Init = ();
    type Input = WatermarkToolMsg;
    type Output = WatermarkToolOutput;

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.watermark_page.widget(), Some("watermark")),

            #[watch]
            set_visible_child_name: if matches!(model.state, ToolState::Ready | ToolState::Processing) { "watermark" } else { "empty" },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let empty_page = ToolPage::builder()
            .launch(Tool::Watermark)
            .forward(sender.input_sender(), WatermarkToolMsg::AddFiles);

        let watermark_page =
            WatermarkPage::builder()
                .launch(())
                .forward(sender.input_sender(), |msg| match msg {
                    WatermarkPageOutput::FileActive(file_stem) => {
                        WatermarkToolMsg::UpdateFileActive(file_stem)
                    }
                    WatermarkPageOutput::Loading(is_loading) => {
                        WatermarkToolMsg::Loading(is_loading)
                    }
                });

        let model = Self {
            state: ToolState::Empty,
            _empty_page: empty_page,
            watermark_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            WatermarkToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.watermark_page.emit(WatermarkPageMsg::AddFile(file));
                }
            }
            WatermarkToolMsg::UpdateFileActive(file_stem) => {
                if file_stem.is_none() {
                    self.state = ToolState::Empty;
                }
                let _ = sender.output(WatermarkToolOutput::FileActive(file_stem));
            }
            WatermarkToolMsg::Loading(is_loading) => {
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
                let _ = sender.output(WatermarkToolOutput::Loading(is_loading));
            }
        }
    }
}

struct WatermarkPage {
    file: Option<gio::File>,
    password: Option<String>,
    image_file: Option<gio::File>,
    layer: WatermarkLayer,
    opacity: u32,
    pages: WatermarkPages,
    specific_pages: String,
    specific_pages_error: Option<String>,
    page_count: u32,

    modern_pdf_format: bool,
    remove_metadata: bool,

    is_saving: bool,
    is_loading: bool,
    thumbnail: Option<gdk::MemoryTexture>,
    password_dialog: Controller<PasswordDialog>,
    preview_status: PreviewStatus,
}

#[derive(Debug)]
enum WatermarkPageMsg {
    AddFile(gio::File),
    SetImageFile(Option<gio::File>),
    SetLayer(WatermarkLayer),
    SetOpacity(u32),
    SetPages(WatermarkPages),
    SetSpecificPages(String),
    SetModernPdfFormat(bool),
    SetRemoveMetadata(bool),
    SaveTo(gio::File),
    SaveComplete(Result<std::path::PathBuf, PdfError>),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    PasswordDialogOutput(PasswordDialogOutput),
    OpenOutput(std::path::PathBuf),
}

#[derive(Debug)]
pub enum WatermarkPageOutput {
    FileActive(Option<String>),
    Loading(bool),
}

fn open_image_dialog(button: &gtk::Button, callback: impl FnOnce(gio::File) + 'static) {
    let image_filter = gtk::FileFilter::new();
    image_filter.set_name(Some(&gettext("Images")));
    image_filter.add_mime_type("image/*");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&image_filter);

    let dialog = gtk::FileDialog::builder()
        .title(gettext("Select Watermark Image"))
        .accept_label(gettext("Select"))
        .modal(true)
        .filters(&filters)
        .build();

    let parent = button.root().and_downcast::<gtk::Window>();
    dialog.open(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
        if let Ok(file) = result {
            callback(file);
        }
    });
}

#[relm4::component]
impl Component for WatermarkPage {
    type Init = ();
    type Input = WatermarkPageMsg;
    type Output = WatermarkPageOutput;
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
                            set_label: &Tool::Watermark.action_label(),
                            set_tooltip_text: Some(&gettext("Select PDF File")),

                            connect_clicked[sender] => move |button| {
                                let sender_clone = sender.clone();
                                open_pdf_dialog(button, Tool::Watermark, move |mut files| {
                                    if let Some(file) = files.pop() {
                                        sender_clone.input(WatermarkPageMsg::AddFile(file));
                                    }
                                });
                            },
                        },

                        gtk::Button {
                            set_label: &gettext("Save"),
                            set_tooltip_text: Some(&gettext("Save watermarked PDF")),
                            add_css_class: "suggested-action",
                            #[watch]
                            set_sensitive: model.file.is_some() && model.image_file.is_some(),

                            connect_clicked[sender] => move |button| {
                                let sender_clone = sender.clone();
                                save_pdf_dialog(button, Tool::Watermark, &gettext("Save PDF"), move |file| {
                                    sender_clone.input(WatermarkPageMsg::SaveTo(file));
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
                                            sender.input(WatermarkPageMsg::SetModernPdfFormat(row.is_active()));
                                        }
                                    },

                                    add = &adw::SwitchRow {
                                        set_title: &gettext("_Remove metadata"),
                                        set_use_underline: true,
                                        set_subtitle: &gettext("Remove existing metadata before saving"),
                                        set_active: model.remove_metadata,

                                        connect_active_notify[sender] => move |row| {
                                            sender.input(WatermarkPageMsg::SetRemoveMetadata(row.is_active()));
                                        }
                                    },
                                }
                            }
                        }
                    },

                    #[name(watermark_box)]
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
                                    add = &adw::ActionRow {
                                        set_title: &gettext("Watermark Image"),
                                        #[watch]
                                        set_subtitle: &model
                                            .image_file
                                            .as_ref()
                                            .map(file_name)
                                            .unwrap_or_else(|| gettext("No image selected")),

                                        add_suffix = &gtk::Button {
                                            set_icon_name: "folder-open-symbolic",
                                            add_css_class: "flat",
                                            set_valign: gtk::Align::Center,
                                            set_tooltip_text: Some(&gettext("Select File")),

                                            connect_clicked[sender] => move |button| {
                                                let sender_clone = sender.clone();
                                                open_image_dialog(button, move |file| {
                                                    sender_clone.input(WatermarkPageMsg::SetImageFile(Some(file)));
                                                });
                                            },
                                        },
                                    },

                                    add = &adw::ComboRow {
                                        set_title: &gettext("Layer"),
                                        set_model: Some(&gtk::StringList::new(&[
                                            gettext("Front").as_str(),
                                            gettext("Back").as_str(),
                                        ])),
                                        set_selected: match model.layer {
                                            WatermarkLayer::Front => 0,
                                            WatermarkLayer::Back => 1,
                                        },
                                        connect_selected_notify[sender] => move |row| {
                                            sender.input(WatermarkPageMsg::SetLayer(WatermarkLayer::from(row.selected())));
                                        }
                                    },

                                    add = &adw::SpinRow {
                                        set_title: &gettext("Opacity (%)"),
                                        set_numeric: true,
                                        #[wrap(Some)]
                                        set_adjustment = &gtk::Adjustment {
                                            set_lower: 0.0,
                                            set_upper: 100.0,
                                            set_value: 100.0,
                                            set_step_increment: 5.0,
                                            set_page_increment: 10.0,
                                        },
                                        connect_value_notify[sender] => move |row| {
                                            sender.input(WatermarkPageMsg::SetOpacity(row.value() as u32));
                                        }
                                    },

                                    add = &adw::ComboRow {
                                        set_title: &gettext("Pages"),
                                        set_model: Some(&gtk::StringList::new(&[
                                            gettext("All pages").as_str(),
                                            gettext("First page").as_str(),
                                            gettext("Last page").as_str(),
                                            gettext("Specific pages").as_str(),
                                        ])),
                                        set_selected: match model.pages {
                                            WatermarkPages::AllPages => 0,
                                            WatermarkPages::FirstPage => 1,
                                            WatermarkPages::LastPage => 2,
                                            WatermarkPages::SpecificPages => 3,
                                        },
                                        connect_selected_notify[sender] => move |row| {
                                            sender.input(WatermarkPageMsg::SetPages(WatermarkPages::from(row.selected())));
                                        }
                                    },

                                    add = &adw::EntryRow {
                                        set_title: &gettext("Pages"),
                                        #[watch]
                                        set_visible: matches!(model.pages, WatermarkPages::SpecificPages),
                                        set_text: &model.specific_pages,
                                        set_show_apply_button: false,
                                        #[watch]
                                        set_class_active: ("error", model.specific_pages_error.is_some()),
                                        connect_changed[sender] => move |entry| {
                                            sender.input(WatermarkPageMsg::SetSpecificPages(entry.text().to_string()));
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
        let password_dialog = PasswordDialog::builder().launch(()).forward(
            sender.input_sender(),
            WatermarkPageMsg::PasswordDialogOutput,
        );

        let model = Self {
            file: None,
            password: None,
            image_file: None,
            layer: WatermarkLayer::Front,
            opacity: 100,
            pages: WatermarkPages::AllPages,
            specific_pages: String::new(),
            specific_pages_error: None,
            page_count: 0,
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
            &widgets.watermark_box,
            "orientation",
            gtk::Orientation::Vertical,
        )]);
        bp.add_setters(&[(&widgets.picture_clamp, "vexpand", true)]);
        widgets.breakpoint_bin.add_breakpoint(bp);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            WatermarkPageMsg::AddFile(file) => {
                self.password = None;
                self.preview_status = PreviewStatus::InitialPending;

                let name = file_name(&file);
                self.file = Some(file.clone());

                self.check_loading_state(&sender);
                let _ = sender.output(WatermarkPageOutput::FileActive(Some(name)));

                self.request_thumbnail(None, &sender);
            }
            WatermarkPageMsg::SetImageFile(file) => {
                self.image_file = file;
                self.request_thumbnail(self.password.clone(), &sender);
            }
            WatermarkPageMsg::SetLayer(layer) => {
                self.layer = layer;
            }
            WatermarkPageMsg::SetOpacity(opacity) => {
                self.opacity = opacity;
                self.request_thumbnail(self.password.clone(), &sender);
            }
            WatermarkPageMsg::SetPages(pages) => {
                self.pages = pages;
            }
            WatermarkPageMsg::SetSpecificPages(pages) => {
                self.specific_pages = pages;
                self.specific_pages_error = None;
            }
            WatermarkPageMsg::SetModernPdfFormat(val) => {
                self.modern_pdf_format = val;
            }
            WatermarkPageMsg::SetRemoveMetadata(val) => {
                self.remove_metadata = val;
            }
            WatermarkPageMsg::SaveTo(output_file) => {
                if let (Some(file_path), Some(image_path), Some(output_path)) = (
                    self.file.as_ref().and_then(|f| f.path()),
                    self.image_file.as_ref().and_then(|f| f.path()),
                    output_file.path(),
                ) {
                    let specific_pages_list = if matches!(self.pages, WatermarkPages::SpecificPages)
                    {
                        match crate::tools::validate_page_ranges(
                            &self.specific_pages,
                            self.page_count,
                        ) {
                            Ok(pages) => pages,
                            Err(err) => {
                                self.specific_pages_error = Some(err);
                                return;
                            }
                        }
                    } else {
                        Vec::new()
                    };

                    self.is_saving = true;
                    self.check_loading_state(&sender);

                    let options = WatermarkOptions {
                        image_path,
                        layer: self.layer,
                        opacity: self.opacity,
                        pages: self.pages,
                        specific_pages: specific_pages_list,
                        modern_pdf_format: self.modern_pdf_format,
                        remove_metadata: self.remove_metadata,
                        password: self.password.clone(),
                    };

                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = watermark_file(&(file_path, 0), output_path.clone(), &options);
                        match result {
                            Ok(_) => sender.input(WatermarkPageMsg::SaveComplete(Ok(output_path))),
                            Err(e) => sender.input(WatermarkPageMsg::SaveComplete(Err(e))),
                        }
                    });
                }
            }
            WatermarkPageMsg::SaveComplete(result) => {
                self.is_saving = false;
                self.check_loading_state(&sender);
                match result {
                    Ok(path) => {
                        tracing::info!("Watermark complete");
                        let toast = adw::Toast::new(&gettext("Watermark added successfully"));
                        toast.set_button_label(Some(&gettext("Open File")));
                        let sender_clone = sender.clone();
                        toast.connect_button_clicked(move |_| {
                            sender_clone.input(WatermarkPageMsg::OpenOutput(path.clone()));
                        });
                        root.add_toast(toast);
                    }
                    Err(err) => {
                        tracing::error!("Failed to add watermark to PDF: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to add watermark to PDF")));
                    }
                }
            }
            WatermarkPageMsg::OpenOutput(path) => {
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
            WatermarkPageMsg::ThumbnailReady(result) => {
                match result {
                    Ok(thumb_res) => {
                        self.thumbnail = thumb_res.texture;
                        self.page_count = thumb_res.page_count as u32;
                        self.preview_status = PreviewStatus::Ready;
                    }
                    Err(PreviewError::Encrypted) => {
                        self.preview_status = PreviewStatus::PasswordRequired;
                        let is_error = self.password.is_some();
                        let filename = self.file.as_ref().map(file_name).unwrap_or_default();
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
                        tracing::warn!(
                            "Failed to generate thumbnail for watermark page: {:?}",
                            err
                        );
                        self.thumbnail = None;
                        self.preview_status = PreviewStatus::Ready;
                    }
                }
                self.check_loading_state(&sender);
            }
            WatermarkPageMsg::PasswordDialogOutput(output) => match output {
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

impl WatermarkPage {
    fn request_thumbnail(&self, password: Option<String>, sender: &ComponentSender<Self>) {
        if let Some(file) = &self.file {
            let sender_clone = sender.clone();
            let file_clone = file.clone();
            let image_file_clone = self.image_file.clone();
            let opacity = self.opacity as f64 / 100.0;

            if let Err(e) = crate::pdf::preview::thread_pool().push(move || {
                let result = crate::pdf::preview::generate_watermark_thumbnail(
                    &file_clone,
                    0,
                    0,
                    password.as_deref(),
                    800.0,
                    image_file_clone.as_ref(),
                    opacity,
                );
                sender_clone.input(WatermarkPageMsg::ThumbnailReady(result));
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
        let _ = sender.output(WatermarkPageOutput::FileActive(None));
    }

    fn check_loading_state(&mut self, sender: &ComponentSender<Self>) {
        let is_loading = self.is_saving
            || matches!(
                self.preview_status,
                PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
            );

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(WatermarkPageOutput::Loading(is_loading));
        }
    }
}
