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
use crate::pdf::{OrganizeOptions, OrganizePageInput, PdfError, organize_file};
use crate::tools::page::ToolPage;
use crate::tools::{PreviewStatus, Tool, ToolState, file_stem, open_pdf_dialog, save_pdf_dialog};

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

relm4::new_action_group!(CardActionGroup, "card");
relm4::new_stateless_action!(MoveLeftAction, CardActionGroup, "move-left");
relm4::new_stateless_action!(MoveRightAction, CardActionGroup, "move-right");
relm4::new_stateless_action!(DuplicateAction, CardActionGroup, "duplicate");
relm4::new_stateless_action!(InsertBlankAction, CardActionGroup, "insert-blank");

#[derive(Debug, Clone)]
pub enum OrganizeItemType {
    Page(usize),
    BlankPage { width: f64, height: f64 },
}

#[derive(Debug, Clone)]
struct OrganizePageRowInit {
    file: gio::File,
    item_type: OrganizeItemType,
    total_pages: usize,
    rotation: u16,
    thumbnail: Option<gdk::MemoryTexture>,
    original_dimensions: Option<(f64, f64)>,
    password: Option<String>,
}

struct OrganizePageRow {
    file: gio::File,
    item_type: OrganizeItemType,
    rotation: u16,
    password: Option<String>,
    thumbnail: Option<gdk::MemoryTexture>,
    original_dimensions: Option<(f64, f64)>,
    index: DynamicIndex,
    total_pages: usize,
    action_group: gio::SimpleActionGroup,
    move_left_action: gio::SimpleAction,
    move_right_action: gio::SimpleAction,
    insert_blank_action: gio::SimpleAction,
}

#[derive(Debug)]
enum OrganizePageRowMsg {
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    RotateClockwise,
    RefreshBounds(usize),
}

#[derive(Debug)]
enum OrganizePageRowOutput {
    MoveLeft(DynamicIndex),
    MoveRight(DynamicIndex),
    Duplicate(DynamicIndex),
    Delete(DynamicIndex),
    InsertBlankPageAfter(DynamicIndex),
    Move { from: usize, to: DynamicIndex },
}

#[relm4::factory]
impl FactoryComponent for OrganizePageRow {
    type Init = OrganizePageRowInit;
    type Input = OrganizePageRowMsg;
    type Output = OrganizePageRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::FlowBox;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_width_request: 160,

            #[name(preview_frame)]
            gtk::Overlay {
                set_margin_top: 12,
                set_margin_bottom: 12,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_width_request: 126,
                    set_height_request: 162,
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
                        let _ = sender.output(OrganizePageRowOutput::Move {
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

            gtk::Label {
                set_label: &match self.item_type {
                    OrganizeItemType::Page(idx) => format!("{} {}", gettext("Page"), idx + 1),
                    OrganizeItemType::BlankPage { .. } => gettext("Blank Page"),
                },
                set_halign: gtk::Align::Start,
                set_margin_start: 12,
                set_margin_end: 12,
                set_margin_top: 4,
                add_css_class: "heading",
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_margin_start: 12,
                set_margin_end: 6,
                set_margin_bottom: 6,

                gtk::Label {
                    #[watch]
                    set_label: &format!("{}/{}", self.index.current_index() + 1, self.total_pages),
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

                gtk::Button {
                    set_icon_name: "edit-delete-symbolic",
                    add_css_class: "flat",
                    set_tooltip_text: Some(&gettext("Remove Page")),
                    #[watch]
                    set_sensitive: self.total_pages > 1,

                    connect_clicked[sender, index] => move |_| {
                        let _ = sender.output(OrganizePageRowOutput::Delete(index.clone()));
                    }
                },

                #[name(menu_button)]
                gtk::MenuButton {
                    set_icon_name: "view-more-symbolic",
                    add_css_class: "flat",
                    set_tooltip_text: Some(&gettext("More Options")),

                    insert_action_group: ("card", Some(&self.action_group)),

                    set_menu_model: Some(&{
                        relm4::menu! {
                            card_menu: {
                                section! {
                                    &gettext("Move _Left") => MoveLeftAction,
                                    &gettext("Move _Right") => MoveRightAction,
                                    &gettext("_Duplicate") => DuplicateAction,
                                    &gettext("_Insert Blank Page After") => InsertBlankAction,
                                }
                            }
                        }
                        card_menu
                    }),
                }
            }
        }
    }

    fn init_model(init: Self::Init, index: &DynamicIndex, sender: FactorySender<Self>) -> Self {
        let action_group = gio::SimpleActionGroup::new();
        let move_left_action = gio::SimpleAction::new("move-left", None);
        let move_right_action = gio::SimpleAction::new("move-right", None);
        let duplicate_action = gio::SimpleAction::new("duplicate", None);
        let insert_blank_action = gio::SimpleAction::new("insert-blank", None);

        let sender_left = sender.clone();
        let index_left = index.clone();
        move_left_action.connect_activate(move |_, _| {
            let _ = sender_left.output(OrganizePageRowOutput::MoveLeft(index_left.clone()));
        });

        let sender_right = sender.clone();
        let index_right = index.clone();
        move_right_action.connect_activate(move |_, _| {
            let _ = sender_right.output(OrganizePageRowOutput::MoveRight(index_right.clone()));
        });

        let sender_dup = sender.clone();
        let index_dup = index.clone();
        duplicate_action.connect_activate(move |_, _| {
            let _ = sender_dup.output(OrganizePageRowOutput::Duplicate(index_dup.clone()));
        });

        let sender_blank = sender.clone();
        let index_blank = index.clone();
        insert_blank_action.connect_activate(move |_, _| {
            let _ = sender_blank.output(OrganizePageRowOutput::InsertBlankPageAfter(
                index_blank.clone(),
            ));
        });

        insert_blank_action.set_enabled(false);

        action_group.add_action(&move_left_action);
        action_group.add_action(&move_right_action);
        action_group.add_action(&duplicate_action);
        action_group.add_action(&insert_blank_action);

        let mut model = Self {
            file: init.file,
            item_type: init.item_type,
            rotation: init.rotation,
            password: init.password,
            thumbnail: init.thumbnail,
            original_dimensions: init.original_dimensions,
            index: index.clone(),
            total_pages: init.total_pages,
            action_group,
            move_left_action,
            move_right_action,
            insert_blank_action,
        };
        model.update_actions();
        if model.original_dimensions.is_some() {
            model.insert_blank_action.set_enabled(true);
        } else if let OrganizeItemType::BlankPage { width, height } = model.item_type {
            model.original_dimensions = Some((width, height));
            model.insert_blank_action.set_enabled(true);
        }
        if model.thumbnail.is_none() {
            model.request_thumbnail(&sender);
        }
        model
    }

    fn update(&mut self, message: Self::Input, sender: FactorySender<Self>) {
        match message {
            OrganizePageRowMsg::ThumbnailReady(res) => {
                if let Ok(thumb) = res {
                    self.thumbnail = thumb.texture;
                    if let Some(dim) = thumb.original_dimensions {
                        self.original_dimensions = Some(dim);
                        self.insert_blank_action.set_enabled(true);
                    }
                }
            }
            OrganizePageRowMsg::RotateClockwise => {
                self.rotation = (self.rotation + 90) % 360;
                self.request_thumbnail(&sender);
            }
            OrganizePageRowMsg::RefreshBounds(total_pages) => {
                self.total_pages = total_pages;
                self.update_actions();
            }
        }
    }
}

impl OrganizePageRow {
    fn update_actions(&self) {
        let pos = self.index.current_index();
        self.move_left_action.set_enabled(pos > 0);
        self.move_right_action
            .set_enabled(pos + 1 < self.total_pages);
    }
}

impl OrganizePageRow {
    fn request_thumbnail(&self, sender: &FactorySender<Self>) {
        let item_type = self.item_type.clone();
        let file = self.file.clone();
        let rotation = self.rotation as i32;
        let password = self.password.clone();
        let sender = sender.clone();

        if let Err(e) = crate::pdf::preview::thread_pool().push(move || {
            let result = match item_type {
                OrganizeItemType::Page(page_index) => crate::pdf::preview::generate_page_thumbnail(
                    &file,
                    page_index as i32,
                    rotation,
                    password.as_deref(),
                    200.0,
                ),
                OrganizeItemType::BlankPage { width, height } => {
                    crate::pdf::preview::generate_blank_thumbnail(width, height, rotation, 200.0)
                }
            };
            sender.input(OrganizePageRowMsg::ThumbnailReady(result));
        }) {
            tracing::error!(
                "Failed to enqueue thumbnail task for organize page row: {}",
                e
            );
        }
    }
}

struct OrganizePage {
    file: Option<gio::File>,
    password: Option<String>,
    is_loading: bool,
    is_saving: bool,
    modern_pdf_format: bool,
    remove_metadata: bool,
    password_dialog: Controller<PasswordDialog>,
    preview_status: PreviewStatus,
    pages: FactoryVecDeque<OrganizePageRow>,
}

#[derive(Debug)]
enum OrganizePageMsg {
    AddFile(gio::File),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
    PasswordDialogOutput(PasswordDialogOutput),
    MovePageLeft(DynamicIndex),
    MovePageRight(DynamicIndex),
    DuplicatePage(DynamicIndex),
    DeletePage(DynamicIndex),
    InsertBlankPageAfter(DynamicIndex),
    MovePage { from: usize, to: DynamicIndex },
    ResetFile,
    SetModernPdfFormat(bool),
    SetRemoveMetadata(bool),
    SaveTo(gio::File),
    SaveComplete(Result<std::path::PathBuf, PdfError>),
    OpenOutput(std::path::PathBuf),
    RotateAll,
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

                    gtk::Button {
                        set_label: &gettext("Reset"),
                        set_tooltip_text: Some(&gettext("Reset Page Order and Rotations")),
                        #[watch]
                        set_sensitive: model.file.is_some(),

                        connect_clicked[sender] => move |_| {
                            sender.input(OrganizePageMsg::ResetFile);
                        },
                    },

                    gtk::Button {
                        set_label: &gettext("Save"),
                        set_tooltip_text: Some(&gettext("Save organized PDF")),
                        add_css_class: "suggested-action",
                        #[watch]
                        set_sensitive: model.file.is_some(),

                        connect_clicked[sender] => move |button| {
                            let sender_clone = sender.clone();
                            save_pdf_dialog(button, Tool::Organize, &gettext("Save PDF"), move |file| {
                                sender_clone.input(OrganizePageMsg::SaveTo(file));
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
                                        sender.input(OrganizePageMsg::RotateAll);
                                    }
                                },

                                add = &adw::SwitchRow {
                                    set_title: &gettext("_Modern PDF format"),
                                    set_use_underline: true,
                                    set_subtitle: &gettext("Save with PDF 1.5 object streams"),
                                    set_active: model.modern_pdf_format,

                                    connect_active_notify[sender] => move |row| {
                                        sender.input(OrganizePageMsg::SetModernPdfFormat(row.is_active()));
                                    }
                                },

                                add = &adw::SwitchRow {
                                    set_title: &gettext("_Remove metadata"),
                                    set_use_underline: true,
                                    set_subtitle: &gettext("Remove existing metadata before saving"),
                                    set_active: model.remove_metadata,

                                    connect_active_notify[sender] => move |row| {
                                        sender.input(OrganizePageMsg::SetRemoveMetadata(row.is_active()));
                                    }
                                },
                            }
                        }
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
            .forward(sender.input_sender(), |output| match output {
                OrganizePageRowOutput::MoveLeft(idx) => OrganizePageMsg::MovePageLeft(idx),
                OrganizePageRowOutput::MoveRight(idx) => OrganizePageMsg::MovePageRight(idx),
                OrganizePageRowOutput::Duplicate(idx) => OrganizePageMsg::DuplicatePage(idx),
                OrganizePageRowOutput::Delete(idx) => OrganizePageMsg::DeletePage(idx),
                OrganizePageRowOutput::InsertBlankPageAfter(idx) => {
                    OrganizePageMsg::InsertBlankPageAfter(idx)
                }
                OrganizePageRowOutput::Move { from, to } => OrganizePageMsg::MovePage { from, to },
            });

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
            is_saving: false,
            modern_pdf_format: false,
            remove_metadata: false,
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
                                        item_type: OrganizeItemType::Page(i as usize),
                                        total_pages: res.page_count as usize,
                                        rotation: 0,
                                        thumbnail: None,
                                        original_dimensions: None,
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
            OrganizePageMsg::MovePageLeft(index) => {
                let current = index.current_index();
                if current > 0 {
                    self.pages.guard().move_to(current, current - 1);
                    self.refresh_range(current - 1, current);
                }
            }
            OrganizePageMsg::MovePageRight(index) => {
                let current = index.current_index();
                let new_index = current + 1;
                if new_index < self.pages.len() {
                    self.pages.guard().move_to(current, new_index);
                    self.refresh_range(current, new_index);
                }
            }
            OrganizePageMsg::DuplicatePage(index) => {
                let current = index.current_index();
                let total = self.pages.len() + 1;
                let prepared = self
                    .pages
                    .guard()
                    .get(current)
                    .map(|row| OrganizePageRowInit {
                        file: row.file.clone(),
                        item_type: row.item_type.clone(),
                        total_pages: total,
                        rotation: row.rotation,
                        thumbnail: row.thumbnail.clone(),
                        original_dimensions: row.original_dimensions,
                        password: row.password.clone(),
                    });
                if let Some(prepared) = prepared {
                    self.pages.guard().insert(current + 1, prepared);
                    self.refresh_all_bounds();
                }
            }
            OrganizePageMsg::InsertBlankPageAfter(index) => {
                let current = index.current_index();
                let total = self.pages.len() + 1;
                let info = self
                    .pages
                    .guard()
                    .get(current)
                    .map(|row| (row.file.clone(), row.original_dimensions, row.rotation));
                if let Some((file, Some((w, h)), rotation)) = info {
                    let prepared = OrganizePageRowInit {
                        file,
                        item_type: OrganizeItemType::BlankPage {
                            width: w,
                            height: h,
                        },
                        total_pages: total,
                        rotation,
                        thumbnail: None,
                        original_dimensions: Some((w, h)),
                        password: None,
                    };
                    self.pages.guard().insert(current + 1, prepared);
                    self.refresh_all_bounds();
                }
            }
            OrganizePageMsg::DeletePage(index) => {
                let current = index.current_index();
                if self.pages.len() > 1 {
                    self.pages.guard().remove(current);
                    self.refresh_all_bounds();
                }
            }
            OrganizePageMsg::MovePage { from, to } => {
                let to_idx = to.current_index();
                if from != to_idx && from < self.pages.len() && to_idx < self.pages.len() {
                    self.pages.guard().move_to(from, to_idx);
                    let start = from.min(to_idx);
                    let end = from.max(to_idx);
                    self.refresh_range(start, end);
                }
            }
            OrganizePageMsg::ResetFile => {
                if self.file.is_some() {
                    self.pages.guard().clear();
                    self.preview_status = PreviewStatus::InitialPending;
                    self.check_loading_state(&sender);
                    self.request_thumbnail(self.password.clone(), &sender);
                }
            }
            OrganizePageMsg::SetModernPdfFormat(val) => {
                self.modern_pdf_format = val;
            }
            OrganizePageMsg::SetRemoveMetadata(val) => {
                self.remove_metadata = val;
            }
            OrganizePageMsg::SaveTo(output_file) => {
                if let (Some(file_path), Some(output_path)) = (
                    self.file.as_ref().and_then(|f| f.path()),
                    output_file.path(),
                ) {
                    self.is_saving = true;
                    self.check_loading_state(&sender);

                    let pages = self
                        .pages
                        .guard()
                        .iter()
                        .map(|row| {
                            let input = match &row.item_type {
                                OrganizeItemType::Page(idx) => OrganizePageInput::Page(*idx),
                                OrganizeItemType::BlankPage { width, height } => {
                                    OrganizePageInput::BlankPage {
                                        width: *width,
                                        height: *height,
                                    }
                                }
                            };
                            (input, row.rotation)
                        })
                        .collect();

                    let options = OrganizeOptions {
                        pages,
                        modern_pdf_format: self.modern_pdf_format,
                        remove_metadata: self.remove_metadata,
                        password: self.password.clone(),
                    };

                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = organize_file(&(file_path, 0), output_path.clone(), &options);
                        match result {
                            Ok(_) => sender.input(OrganizePageMsg::SaveComplete(Ok(output_path))),
                            Err(e) => sender.input(OrganizePageMsg::SaveComplete(Err(e))),
                        }
                    });
                }
            }
            OrganizePageMsg::SaveComplete(result) => {
                self.is_saving = false;
                self.check_loading_state(&sender);
                match result {
                    Ok(path) => {
                        tracing::info!("Organize complete");
                        let toast = adw::Toast::new(&gettext("PDF organized successfully"));
                        toast.set_button_label(Some(&gettext("Open File")));
                        let sender_clone = sender.clone();
                        toast.connect_button_clicked(move |_| {
                            sender_clone.input(OrganizePageMsg::OpenOutput(path.clone()));
                        });
                        root.add_toast(toast);
                    }
                    Err(err) => {
                        tracing::error!("Failed to organize PDF: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to organize PDF")));
                    }
                }
            }
            OrganizePageMsg::OpenOutput(path) => {
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
            OrganizePageMsg::RotateAll => {
                for i in 0..self.pages.len() {
                    self.pages.send(i, OrganizePageRowMsg::RotateClockwise);
                }
            }
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
        let is_loading = self.is_saving
            || matches!(
                self.preview_status,
                PreviewStatus::InitialPending | PreviewStatus::PasswordRequired
            );

        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            let _ = sender.output(OrganizePageOutput::Loading(is_loading));
        }
    }

    fn refresh_range(&mut self, start: usize, end: usize) {
        let length = self.pages.len();
        let guard = self.pages.guard();
        for i in start..=end {
            if i < length {
                guard.send(i, OrganizePageRowMsg::RefreshBounds(length));
            }
        }
    }

    fn refresh_all_bounds(&mut self) {
        let length = self.pages.len();
        if length > 0 {
            self.refresh_range(0, length - 1);
        }
    }
}
