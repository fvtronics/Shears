use gettextrs::gettext;
use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, RelmWidgetExt,
    SimpleComponent, adw, gtk,
};

use gtk::{gdk, gio};

use crate::pdf::preview::PreviewError;
use crate::pdf::{DivideAfter, PdfError, SplitOptions, split_file};
use crate::tools::page::ToolPage;
use crate::tools::{Tool, open_pdf_dialog, select_folder_dialog};

pub struct SplitTool {
    has_file: bool,
    _empty_page: Controller<ToolPage>,
    split_page: Controller<SplitPage>,
}

#[derive(Debug)]
pub enum SplitToolMsg {
    AddFiles(Vec<gio::File>),
}

#[relm4::component(pub)]
impl SimpleComponent for SplitTool {
    type Init = ();
    type Input = SplitToolMsg;
    type Output = ();

    view! {
        gtk::Stack {
            set_vhomogeneous: false,

            add_named: (model._empty_page.widget(), Some("empty")),
            add_named: (model.split_page.widget(), Some("split")),

            #[watch]
            set_visible_child_name: if model.has_file { "split" } else { "empty" },
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

        let split_page = SplitPage::builder().launch(()).detach();

        let model = Self {
            has_file: false,
            _empty_page: empty_page,
            split_page,
        };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            SplitToolMsg::AddFiles(mut files) => {
                if let Some(file) = files.pop() {
                    self.split_page.emit(SplitPageMsg::AddFile(file));
                    self.has_file = true;
                }
            }
        }
    }
}

struct SplitPage {
    file: Option<gio::File>,
    prefix: String,
    prefix_changed: bool,
    divide_after: DivideAfter,
    is_splitting: bool,
    thumbnail: Option<gdk::MemoryTexture>,
}

#[derive(Debug)]
enum SplitPageMsg {
    AddFile(gio::File),
    SetDivideAfter(DivideAfter),
    SetPrefix(String),
    SplitTo(gio::File),
    SplitComplete(Result<(), PdfError>),
    ThumbnailReady(Result<crate::pdf::preview::ThumbnailResult, PreviewError>),
}

#[relm4::component]
impl Component for SplitPage {
    type Init = ();
    type Input = SplitPageMsg;
    type Output = ();
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
                        set_spacing: 6,

                        gtk::Box {
                            set_hexpand: true,
                        },

                        gtk::Button {
                            set_label: &Tool::Split.action_label(),
                            set_tooltip_text: Some(&Tool::Split.action_label()),
                            set_halign: gtk::Align::End,
                            #[watch]
                            set_sensitive: !model.is_splitting,

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
                            set_halign: gtk::Align::End,
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
                    },

                    #[name(split_box)]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 24,
                        set_margin_all: 24,

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
                                ])),
                                        connect_selected_notify[sender] => move |row| {
                                            let divide_after = match row.selected() {
                                                0 => DivideAfter::EachPage,
                                                1 => DivideAfter::EvenPages,
                                                _ => DivideAfter::OddPages,
                                            };
                                            sender.input(SplitPageMsg::SetDivideAfter(divide_after));
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
        let model = Self {
            file: None,
            prefix: gettext("output_file"),
            prefix_changed: true,
            divide_after: DivideAfter::EachPage,
            is_splitting: false,
            thumbnail: None,
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
                self.thumbnail = None;

                let sender_clone = sender.clone();
                let file_clone = file.clone();

                if let Err(e) = crate::pdf::preview::thread_pool().push(move || {
                    let result =
                        crate::pdf::preview::generate_thumbnail(&file_clone, 0, None, 800.0);
                    sender_clone.input(SplitPageMsg::ThumbnailReady(result));
                }) {
                    tracing::error!("Failed to enqueue thumbnail task: {}", e);
                }

                self.file = Some(file);
            }

            SplitPageMsg::SetDivideAfter(divide_after) => {
                self.divide_after = divide_after;
            }
            SplitPageMsg::SetPrefix(prefix) => {
                self.prefix = prefix;
            }
            SplitPageMsg::SplitTo(output_folder) => {
                if let (Some(file_path), Some(output_path)) = (
                    self.file.as_ref().and_then(|f| f.path()),
                    output_folder.path(),
                ) {
                    self.is_splitting = true;
                    let options = SplitOptions {
                        divide_after: self.divide_after,
                        prefix: self.prefix.clone(),
                    };
                    let sender = sender.clone();
                    relm4::spawn_blocking(move || {
                        let result = split_file(&(file_path, 0), output_path, &options);
                        sender.input(SplitPageMsg::SplitComplete(result));
                    });
                }
            }
            SplitPageMsg::SplitComplete(result) => {
                self.is_splitting = false;
                match result {
                    Ok(_) => {
                        tracing::info!("Split PDF complete");
                        root.add_toast(adw::Toast::new(&gettext("Split PDFs saved")));
                    }
                    Err(err) => {
                        tracing::error!("Failed to split PDF: {:?}", err);
                        root.add_toast(adw::Toast::new(&gettext("Failed to split PDF")));
                    }
                }
            }
            SplitPageMsg::ThumbnailReady(result) => match result {
                Ok(thumb_res) => {
                    self.thumbnail = thumb_res.texture;
                }
                Err(err) => {
                    tracing::warn!("Failed to generate thumbnail for split page: {:?}", err);
                }
            },
        }
    }
}

fn file_stem(file: &gio::File) -> String {
    file.basename()
        .and_then(|name| {
            std::path::Path::new(&name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| file.uri().to_string())
}
