/* tools/organize.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio};

use crate::modals::password::{PasswordDialog, PasswordDialogMsg, PasswordDialogOutput};
use crate::pdf::preview::PreviewError;
use crate::tools::page::ToolPage;
use crate::tools::{PreviewStatus, Tool, ToolState, file_stem, open_pdf_dialog};

pub struct OrganizeTool {
    state: ToolState,
    _empty_page: Controller<ToolPage>,
    organize_page: Controller<OrganizePage>,
}

#[derive(Debug)]
pub enum OrganizeToolMsg {
    AddFiles(Vec<gio::File>),
    UpdateFileActive(Option<String>),
    Loading(bool),
}

#[derive(Debug)]
pub enum OrganizeToolOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for OrganizeTool {
    type Init = ();
    type Input = OrganizeToolMsg;
    type Output = OrganizeToolOutput;

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.organize_page.widget(), Some("organize")),

            #[watch]
            set_visible_child_name: if matches!(model.state, ToolState::Ready | ToolState::Processing) { "organize" } else { "empty" },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let empty_page = ToolPage::builder()
            .launch(Tool::Organize)
            .forward(sender.input_sender(), OrganizeToolMsg::AddFiles);

        let organize_page =
            OrganizePage::builder()
                .launch(())
                .forward(sender.input_sender(), |msg| match msg {
                    OrganizePageOutput::FileActive(file_stem) => {
                        OrganizeToolMsg::UpdateFileActive(file_stem)
                    }
                    OrganizePageOutput::Loading(is_loading) => OrganizeToolMsg::Loading(is_loading),
                });

        let model = Self {
            state: ToolState::Empty,
            _empty_page: empty_page,
            organize_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>) {
        match message {
            OrganizeToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.organize_page.emit(OrganizePageMsg::AddFile(file));
                }
            }
            OrganizeToolMsg::UpdateFileActive(file_stem) => {
                if file_stem.is_none() {
                    self.state = ToolState::Empty;
                }
                let _ = sender.output(OrganizeToolOutput::FileActive(file_stem));
            }
            OrganizeToolMsg::Loading(is_loading) => {
                if is_loading {
                    if self.state == ToolState::Empty {
                        self.state = ToolState::LoadingNewFile;
                    } else if self.state == ToolState::Ready {
                        self.state = ToolState::Processing;
                    }
                } else if self.state == ToolState::LoadingNewFile
                    || self.state == ToolState::Processing
                {
                    self.state = ToolState::Ready;
                }
                self._empty_page.emit(is_loading);
                let _ = sender.output(OrganizeToolOutput::Loading(is_loading));
            }
        }
    }
}

#[derive(Debug, Clone)]
struct OrganizePageRowInit {
    file: gio::File,
    page_index: usize,
    password: Option<String>,
}

struct OrganizePageRow {
    file: gio::File,
    page_index: usize,
    rotation: u16,
    password: Option<String>,
    thumbnail: Option<gdk::MemoryTexture>,
}

#[derive(Debug)]
enum OrganizePageRowMsg {
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    RotateClockwise,
}

#[relm4::factory]
impl FactoryComponent for OrganizePageRow {
    type Init = OrganizePageRowInit;
    type Input = OrganizePageRowMsg;
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::FlowBox;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_width_request: 160,

            gtk::Overlay {
                set_margin_top: 12,
                set_margin_bottom: 12,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_width_request: 140,
                    set_height_request: 180,
                },
                add_overlay = &gtk::Picture {
                    set_can_shrink: true,
                    set_content_fit: gtk::ContentFit::Contain,
                    #[watch]
                    set_paintable: self.thumbnail.as_ref(),
                }
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_margin_start: 12,
                set_margin_end: 6,
                set_margin_bottom: 6,

                gtk::Label {
                    set_label: &format!("{}", self.page_index + 1),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    add_css_class: "dim-label",
                },

                gtk::Button {
                    set_icon_name: "object-rotate-right-symbolic",
                    add_css_class: "flat",
                    set_tooltip_text: Some(&gettext("Rotate Clockwise")),
                    connect_clicked => OrganizePageRowMsg::RotateClockwise,
                },

                gtk::MenuButton {
                    set_icon_name: "view-more-symbolic",
                    add_css_class: "flat",
                    set_tooltip_text: Some(&gettext("More Options")),
                }
            }
        }
    }

    fn init_model(
        init: Self::Init,
        _index: &DynamicIndex,
        sender: FactorySender<Self>,
    ) -> Self {
        let model = Self {
            file: init.file,
            page_index: init.page_index,
            rotation: 0,
            password: init.password,
            thumbnail: None,
        };
        model.request_thumbnail(&sender);
        model
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            OrganizePageRowMsg::ThumbnailReady(res) => {
                if let Ok(thumb) = res {
                    self.thumbnail = thumb.texture;
                }
            }
            OrganizePageRowMsg::RotateClockwise => {
                self.rotation = (self.rotation + 90) % 360;
                self.request_thumbnail(&sender);
            }
        }
    }
}

impl OrganizePageRow {
    fn request_thumbnail(&self, sender: &FactorySender<Self>) {
        let file = self.file.clone();
        let page_index = self.page_index as i32;
        let rotation = self.rotation as i32;
        let password = self.password.clone();
        let sender = sender.clone();

        if let Err(e) = crate::pdf::preview::thread_pool().push(move || {
            let result = crate::pdf::preview::generate_page_thumbnail(
                &file,
                page_index,
                rotation,
                password.as_deref(),
                200.0,
            );
            sender.input(OrganizePageRowMsg::ThumbnailReady(result));
        }) {
            tracing::error!("Failed to enqueue thumbnail task for organize page row: {}", e);
        }
    }
}

struct OrganizePage {
    file: Option<gio::File>,
    password: Option<String>,
    is_loading: bool,
    password_dialog: Controller<PasswordDialog>,
    preview_status: PreviewStatus,
    pages: FactoryVecDeque<OrganizePageRow>,
}

#[derive(Debug)]
enum OrganizePageMsg {
    AddFile(gio::File),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    PasswordDialogOutput(PasswordDialogOutput),
}

#[derive(Debug)]
pub enum OrganizePageOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[relm4::component]
impl Component for OrganizePage {
    type Init = ();
    type Input = OrganizePageMsg;
    type Output = OrganizePageOutput;
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
                        set_label: &Tool::Organize.action_label(),
                        set_tooltip_text: Some(&gettext("Select PDF File")),

                        connect_clicked[sender] => move |button| {
                            let sender_clone = sender.clone();
                            open_pdf_dialog(button, Tool::Organize, move |mut files| {
                                if let Some(file) = files.pop() {
                                    sender_clone.input(OrganizePageMsg::AddFile(file));
                                }
                            });
                        },
                    },
                },

                gtk::ScrolledWindow {
                    set_vexpand: true,

                    #[wrap(Some)]
                    set_child = model.pages.widget(),
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let password_dialog = PasswordDialog::builder()
            .launch(())
            .forward(sender.input_sender(), OrganizePageMsg::PasswordDialogOutput);

        let pages = FactoryVecDeque::<OrganizePageRow>::builder()
            .launch(gtk::FlowBox::default())
            .detach();

        pages.widget().set_selection_mode(gtk::SelectionMode::None);
        pages.widget().set_homogeneous(true);
        pages.widget().set_row_spacing(12);
        pages.widget().set_column_spacing(12);
        pages.widget().set_margin_all(12);
        pages.widget().set_valign(gtk::Align::Start);

        let model = Self {
            file: None,
            password: None,
            is_loading: false,
            password_dialog,
            preview_status: PreviewStatus::Ready,
            pages,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        match message {
            OrganizePageMsg::AddFile(file) => {
                self.password = None;
                self.preview_status = PreviewStatus::InitialPending;
                self.pages.guard().clear();

                let stem = file_stem(&file);
                self.file = Some(file.clone());

                self.check_loading_state(&sender);
                let _ = sender.output(OrganizePageOutput::FileActive(Some(stem)));

                self.request_thumbnail(None, &sender);
            }
            OrganizePageMsg::ThumbnailReady(result) => {
                match result {
                    Ok(res) => {
                        self.preview_status = PreviewStatus::Ready;
                        if let Some(file) = &self.file {
                            let mut guard = self.pages.guard();
                            if guard.is_empty() {
                                for i in 0..res.page_count {
                                    guard.push_back(OrganizePageRowInit {
                                        file: file.clone(),
                                        page_index: i as usize,
                                        password: self.password.clone(),
                                    });
                                }
                            }
                        }
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
                        tracing::warn!("Failed to generate thumbnail for organize page: {:?}", err);
                        self.preview_status = PreviewStatus::Ready;
                    }
                }
                self.check_loading_state(&sender);
            }
            OrganizePageMsg::PasswordDialogOutput(output) => match output {
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

impl OrganizePage {
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
                sender_clone.input(OrganizePageMsg::ThumbnailReady(result));
            }) {
                tracing::error!("Failed to enqueue thumbnail task: {}", e);
            }
        }
    }

    fn clear_file(&mut self, sender: &ComponentSender<Self>) {
        self.file = None;
        self.password = None;
        self.preview_status = PreviewStatus::Ready;
        self.pages.guard().clear();
        self.check_loading_state(sender);
        let _ = sender.output(OrganizePageOutput::FileActive(None));
    }

    fn check_loading_state(&mut self, sender: &ComponentSender<Self>) {
        let is_loading = matches!(
            self.preview_status,
            PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
        );

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(OrganizePageOutput::Loading(is_loading));
        }
    }
}
