use std::collections::HashMap;

use gettextrs::gettext;
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
    Tool, ToolOutput, compress::CompressTool, extract::ExtractTool, merge::MergeTool,
    metadata::MetadataTool, organize::OrganizeTool, split::SplitTool, watermark::WatermarkTool,
};

#[derive(Debug, Default, Clone)]
struct ToolStatus {
    is_loading: bool,
    subtitle: Option<String>,
}

pub(super) struct App {
    selected_tool: Tool,
    tool_status: HashMap<Tool, ToolStatus>,
    _merge: Controller<MergeTool>,
    _organize: Controller<OrganizeTool>,
    _extract: Controller<ExtractTool>,
    _split: Controller<SplitTool>,
    _compress: Controller<CompressTool>,
    _watermark: Controller<WatermarkTool>,
    _metadata: Controller<MetadataTool>,
}

#[derive(Debug)]
pub(super) enum AppMsg {
    SelectTool(Tool),
    UpdateToolLoading(Tool, bool),
    UpdateToolSubtitle(Tool, Option<String>),
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
                "_About Shears" => AboutAction,
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
                                        set_title: &gettext("Shears"),
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
        let forward_tool = |tool: Tool| {
            move |msg| match msg {
                ToolOutput::Loading(is_loading) => AppMsg::UpdateToolLoading(tool, is_loading),
                ToolOutput::Subtitle(subtitle) => AppMsg::UpdateToolSubtitle(tool, subtitle),
            }
        };
        let merge = MergeTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Merge));
        let organize = OrganizeTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Organize));
        let extract = ExtractTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Extract));
        let split = SplitTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Split));
        let compress = CompressTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Compress));
        let watermark = WatermarkTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Watermark));
        let metadata = MetadataTool::builder()
            .launch(())
            .forward(sender.input_sender(), forward_tool(Tool::Metadata));

        let model = Self {
            selected_tool: Tool::Merge,
            tool_status: HashMap::new(),
            _merge: merge,
            _organize: organize,
            _extract: extract,
            _split: split,
            _compress: compress,
            _watermark: watermark,
            _metadata: metadata,
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
            AppMsg::UpdateToolLoading(tool, is_loading) => {
                self.tool_status.entry(tool).or_default().is_loading = is_loading;
            }
            AppMsg::UpdateToolSubtitle(tool, subtitle) => {
                self.tool_status.entry(tool).or_default().subtitle = subtitle;
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
        let status = self.tool_status.get(&self.selected_tool);
        if status.is_some_and(|s| s.is_loading) {
            gettext("Processing…")
        } else if let Some(subtitle) = status.and_then(|s| s.subtitle.as_ref()) {
            subtitle.clone()
        } else {
            self.selected_tool.subtitle()
        }
    }
}
