use gettextrs::{gettext, ngettext};
use relm4::adw::prelude::{AdwApplicationWindowExt, IsA, NavigationPageExt, SidebarItemExt};
use relm4::{
    Component, ComponentController, ComponentParts, ComponentSender, Controller, SimpleComponent,
    actions::{AccelsPlus, RelmAction, RelmActionGroup},
    adw, gtk, main_application,
};

use gtk::prelude::{ApplicationExt, GtkWindowExt, OrientableExt, SettingsExt, WidgetExt};
use gtk::{gio, glib};

use crate::config::{APP_ID, PROFILE};
use crate::modals::{about::AboutDialog, shortcuts::ShortcutsDialog};
use crate::tools::{
    Tool, compress::CompressTool, merge::MergeTool, metadata::MetadataTool, organize::OrganizeTool,
    page::ToolPage, split::SplitTool,
};

pub(super) struct App {
    selected_tool: Tool,
    merge_file_count: usize,
    _merge: Controller<MergeTool>,
    _organize: Controller<OrganizeTool>,
    _extract: Controller<ToolPage>,
    _split: Controller<SplitTool>,
    _compress: Controller<CompressTool>,
    _watermark: Controller<ToolPage>,
    _metadata: Controller<MetadataTool>,
    merge_is_loading: bool,
    split_is_loading: bool,
    split_file_stem: Option<String>,
    metadata_is_loading: bool,
    metadata_file_active: Option<String>,
    compress_is_loading: bool,
    compress_file_active: Option<String>,
    organize_is_loading: bool,
    organize_file_active: Option<String>,
}

#[derive(Debug)]
pub(super) enum AppMsg {
    SelectTool(Tool),
    UpdateMergeFileCount(usize),
    UpdateMergeLoading(bool),
    UpdateSplitLoading(bool),
    UpdateSplitFileStem(Option<String>),
    UpdateMetadataLoading(bool),
    UpdateMetadataFileActive(Option<String>),
    UpdateCompressLoading(bool),
    UpdateCompressFileActive(Option<String>),
    UpdateOrganizeLoading(bool),
    UpdateOrganizeFileActive(Option<String>),
    Quit,
}

relm4::new_action_group!(pub(super) WindowActionGroup, "win");
relm4::new_stateless_action!(PreferencesAction, WindowActionGroup, "preferences");
relm4::new_stateless_action!(pub(super) ShortcutsAction, WindowActionGroup, "show-help-overlay");
relm4::new_stateless_action!(AboutAction, WindowActionGroup, "about");
relm4::new_stateless_action!(QuitAction, WindowActionGroup, "quit");

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type Widgets = AppWidgets;

    menu! {
        primary_menu: {
            section! {
                "_Preferences" => PreferencesAction,
                "_Keyboard" => ShortcutsAction,
                "_About Quire" => AboutAction,
            }
        }
    }

    view! {
        #[root]
        main_window = adw::ApplicationWindow::new(&main_application()) {
            set_visible: true,

            connect_close_request[sender] => move |_| {
                sender.input(AppMsg::Quit);
                glib::Propagation::Stop
            },

            add_css_class?: if PROFILE == "Devel" {
                    Some("devel")
                } else {
                    None
                },

            #[name(split_view)]
            adw::NavigationSplitView {

                #[wrap(Some)]
                set_sidebar =
                    &adw::NavigationPage {
                        set_title: &gettext("Sidebar"),

                        #[wrap(Some)]
                        set_child =
                            &adw::ToolbarView {
                                add_top_bar = &adw::HeaderBar {
                                    #[wrap(Some)]
                                    set_title_widget = &adw::WindowTitle {
                                        #[watch]
                                        set_title: &gettext("Quire"),
                                    },

                                    pack_end = &gtk::MenuButton {
                                        set_icon_name: "open-menu-symbolic",
                                        set_menu_model: Some(&primary_menu),
                                    }
                                },

                                #[wrap(Some)]
                                set_content = &adw::Sidebar {
                                    set_selected: 0,

                                    connect_selected_notify[sender, split_view] => move |sidebar| {
                                        sender.input(AppMsg::SelectTool(Tool::from_index(sidebar.selected())));
                                        split_view.set_show_content(true);
                                    },

                                    append = adw::SidebarSection {
                                        append = adw::SidebarItem::new(&gettext("Merge PDFs")) {
                                            set_icon_name: Some(Tool::Merge.icon_name()),
                                            set_subtitle: Some(gettext("Combine files").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Organize Pages")) {
                                            set_icon_name: Some(Tool::Organize.icon_name()),
                                            set_subtitle: Some(gettext("Reorder or remove").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Extract Pages")) {
                                            set_icon_name: Some(Tool::Extract.icon_name()),
                                            set_subtitle: Some(gettext("Save page ranges").as_str()),
                                            set_visible: false,
                                        },

                                        append = adw::SidebarItem::new(&gettext("Split PDF")) {
                                            set_icon_name: Some(Tool::Split.icon_name()),
                                            set_subtitle: Some(gettext("Create separate files").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Compress PDF")) {
                                            set_icon_name: Some(Tool::Compress.icon_name()),
                                            set_subtitle: Some(gettext("Reduce file size").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Add Watermark")) {
                                            set_icon_name: Some(Tool::Watermark.icon_name()),
                                            set_subtitle: Some(gettext("Overlay an image").as_str()),
                                            set_visible: false,
                                        },

                                        append = adw::SidebarItem::new(&gettext("Edit Metadata")) {
                                            set_icon_name: Some(Tool::Metadata.icon_name()),
                                            set_subtitle: Some(gettext("Update document details").as_str()),
                                        },
                                    }
                                }
                            }
                    },

                #[wrap(Some)]
                set_content =
                    &adw::NavigationPage {
                        #[watch]
                        set_title: &model.selected_tool.title(),

                        #[wrap(Some)]
                        set_child =
                            &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,

                                adw::HeaderBar {
                                    #[wrap(Some)]
                                    set_title_widget = &adw::WindowTitle {
                                        #[watch]
                                        set_title: &model.selected_tool.title(),

                                        #[watch]
                                        set_subtitle: &model.current_subtitle(),
                                    },
                                },

                                gtk::Stack {
                                    set_vhomogeneous: false,

                                    add_named: (model._merge.widget(), Some(Tool::Merge.stack_name())),
                                    add_named: (model._organize.widget(), Some(Tool::Organize.stack_name())),
                                    add_named: (model._extract.widget(), Some(Tool::Extract.stack_name())),
                                    add_named: (model._split.widget(), Some(Tool::Split.stack_name())),
                                    add_named: (model._compress.widget(), Some(Tool::Compress.stack_name())),
                                    add_named: (model._watermark.widget(), Some(Tool::Watermark.stack_name())),
                                    add_named: (model._metadata.widget(), Some(Tool::Metadata.stack_name())),

                                    #[watch]
                                    set_visible_child_name: model.selected_tool.stack_name(),
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
        let merge =
            MergeTool::builder()
                .launch(())
                .forward(sender.input_sender(), |msg| match msg {
                    crate::tools::merge::MergeToolOutput::FileCountChanged(len) => {
                        AppMsg::UpdateMergeFileCount(len)
                    }
                    crate::tools::merge::MergeToolOutput::Loading(is_loading) => {
                        AppMsg::UpdateMergeLoading(is_loading)
                    }
                });
        let organize = OrganizeTool::builder()
            .launch(())
            .forward(sender.input_sender(), |msg| match msg {
                crate::tools::organize::OrganizeToolOutput::Loading(is_loading) => {
                    AppMsg::UpdateOrganizeLoading(is_loading)
                }
                crate::tools::organize::OrganizeToolOutput::FileActive(stem) => {
                    AppMsg::UpdateOrganizeFileActive(stem)
                }
            });
        let extract = ToolPage::builder().launch(Tool::Extract).detach();
        let split =
            SplitTool::builder()
                .launch(())
                .forward(sender.input_sender(), |msg| match msg {
                    crate::tools::split::SplitToolOutput::Loading(is_loading) => {
                        AppMsg::UpdateSplitLoading(is_loading)
                    }
                    crate::tools::split::SplitToolOutput::FileActive(stem) => {
                        AppMsg::UpdateSplitFileStem(stem)
                    }
                });
        let compress = CompressTool::builder()
            .launch(())
            .forward(sender.input_sender(), |msg| match msg {
                crate::tools::compress::CompressToolOutput::Loading(is_loading) => {
                    AppMsg::UpdateCompressLoading(is_loading)
                }
                crate::tools::compress::CompressToolOutput::FileActive(stem) => {
                    AppMsg::UpdateCompressFileActive(stem)
                }
            });
        let watermark = ToolPage::builder().launch(Tool::Watermark).detach();
        let metadata = MetadataTool::builder()
            .launch(())
            .forward(sender.input_sender(), |msg| match msg {
                crate::tools::metadata::MetadataToolOutput::Loading(is_loading) => {
                    AppMsg::UpdateMetadataLoading(is_loading)
                }
                crate::tools::metadata::MetadataToolOutput::FileActive(stem) => {
                    AppMsg::UpdateMetadataFileActive(stem)
                }
            });

        let model = Self {
            selected_tool: Tool::Merge,
            merge_file_count: 0,
            _merge: merge,
            _organize: organize,
            _extract: extract,
            _split: split,
            _compress: compress,
            _watermark: watermark,
            _metadata: metadata,
            merge_is_loading: false,
            split_is_loading: false,
            split_file_stem: None,
            metadata_is_loading: false,
            metadata_file_active: None,
            compress_is_loading: false,
            compress_file_active: None,
            organize_is_loading: false,
            organize_file_active: None,
        };
        let widgets = view_output!();
        widgets
            .main_window
            .add_breakpoint(split_view_breakpoint(&widgets.split_view));

        let app = root.application().unwrap();
        let mut actions = RelmActionGroup::<WindowActionGroup>::new();

        let shortcuts_action = {
            RelmAction::<ShortcutsAction>::new_stateless(move |_| {
                ShortcutsDialog::builder().launch(()).detach();
            })
        };

        let about_action = {
            RelmAction::<AboutAction>::new_stateless(move |_| {
                AboutDialog::builder().launch(()).detach();
            })
        };

        let quit_action = {
            RelmAction::<QuitAction>::new_stateless(move |_| {
                sender.input(AppMsg::Quit);
            })
        };

        // Connect action with hotkeys
        app.set_accelerators_for_action::<QuitAction>(&["<Control>q"]);

        actions.add_action(shortcuts_action);
        actions.add_action(about_action);
        actions.add_action(quit_action);
        actions.register_for_widget(&widgets.main_window);

        widgets.load_window_size();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            AppMsg::SelectTool(tool) => {
                self.selected_tool = tool;
            }
            AppMsg::UpdateMergeFileCount(len) => {
                self.merge_file_count = len;
            }
            AppMsg::UpdateMergeLoading(is_loading) => {
                self.merge_is_loading = is_loading;
            }
            AppMsg::UpdateSplitLoading(is_loading) => {
                self.split_is_loading = is_loading;
            }
            AppMsg::UpdateSplitFileStem(stem) => {
                self.split_file_stem = stem;
            }
            AppMsg::UpdateMetadataLoading(is_loading) => {
                self.metadata_is_loading = is_loading;
            }
            AppMsg::UpdateMetadataFileActive(title) => {
                self.metadata_file_active = title;
            }
            AppMsg::UpdateCompressLoading(is_loading) => {
                self.compress_is_loading = is_loading;
            }
            AppMsg::UpdateCompressFileActive(title) => {
                self.compress_file_active = title;
            }
            AppMsg::UpdateOrganizeLoading(is_loading) => {
                self.organize_is_loading = is_loading;
            }
            AppMsg::UpdateOrganizeFileActive(title) => {
                self.organize_file_active = title;
            }
            AppMsg::Quit => main_application().quit(),
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        widgets.save_window_size().unwrap();
    }
}

fn split_view_breakpoint(split_view: &impl IsA<glib::Object>) -> adw::Breakpoint {
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        700.0,
        adw::LengthUnit::Sp,
    ));

    bp.add_setters(&[(split_view, "collapsed", true)]);
    bp
}

impl AppWidgets {
    fn save_window_size(&self) -> Result<(), glib::BoolError> {
        let settings = gio::Settings::new(APP_ID);
        let (width, height) = self.main_window.default_size();

        settings.set_int("window-width", width)?;
        settings.set_int("window-height", height)?;

        settings.set_boolean("is-maximized", self.main_window.is_maximized())?;

        Ok(())
    }

    fn load_window_size(&self) {
        let settings = gio::Settings::new(APP_ID);

        let width = settings.int("window-width");
        let height = settings.int("window-height");
        let is_maximized = settings.boolean("is-maximized");

        self.main_window.set_default_size(width, height);

        if is_maximized {
            self.main_window.maximize();
        }
    }
}

impl App {
    fn current_subtitle(&self) -> String {
        match self.selected_tool {
            Tool::Merge => {
                if self.merge_is_loading {
                    gettext("Processing…")
                } else if self.merge_file_count == 0 {
                    self.selected_tool.subtitle()
                } else {
                    let count = self.merge_file_count as u32;
                    ngettext("{count} file selected", "{count} files selected", count)
                        .replace("{count}", &count.to_string())
                }
            }
            Tool::Split => {
                if self.split_is_loading {
                    gettext("Processing…")
                } else if let Some(stem) = &self.split_file_stem {
                    stem.clone()
                } else {
                    self.selected_tool.subtitle()
                }
            }
            Tool::Metadata => {
                if self.metadata_is_loading {
                    gettext("Processing…")
                } else if let Some(title) = &self.metadata_file_active {
                    title.clone()
                } else {
                    self.selected_tool.subtitle()
                }
            }
            Tool::Compress => {
                if self.compress_is_loading {
                    gettext("Processing…")
                } else if let Some(title) = &self.compress_file_active {
                    title.clone()
                } else {
                    self.selected_tool.subtitle()
                }
            }
            Tool::Organize => {
                if self.organize_is_loading {
                    gettext("Processing…")
                } else if let Some(title) = &self.organize_file_active {
                    title.clone()
                } else {
                    self.selected_tool.subtitle()
                }
            }
            tool => tool.subtitle(),
        }
    }
}
