use gettextrs::gettext;
use relm4::adw::prelude::{NavigationPageExt, SidebarItemExt};
use relm4::{
    Component, ComponentParts, ComponentSender, SimpleComponent,
    actions::{AccelsPlus, RelmAction, RelmActionGroup},
    adw, gtk, main_application,
};

use gtk::prelude::{ApplicationExt, GtkWindowExt, OrientableExt, SettingsExt, WidgetExt};
use gtk::{gio, glib};

use crate::config::{APP_ID, PROFILE};
use crate::modals::{about::AboutDialog, shortcuts::ShortcutsDialog};

pub(super) struct App {}

#[derive(Debug)]
pub(super) enum AppMsg {
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

            adw::NavigationSplitView {

                #[wrap(Some)]
                set_sidebar =
                    &adw::NavigationPage {
                        set_title: &gettext("Sidebar"),

                        #[wrap(Some)]
                        set_child =
                            &adw::ToolbarView {
                                add_top_bar = &adw::HeaderBar {},

                                #[wrap(Some)]
                                set_content = &adw::Sidebar {
                                    set_selected: 0,

                                    append = adw::SidebarSection {
                                        append = adw::SidebarItem::new(&gettext("Merge PDFs")) {
                                            set_icon_name: Some("view-paged-symbolic"),
                                            set_subtitle: Some(gettext("Combine files").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Organize Pages")) {
                                            set_icon_name: Some("view-grid-symbolic"),
                                            set_subtitle: Some(gettext("Reorder or remove").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Extract Pages")) {
                                            set_icon_name: Some("edit-copy-symbolic"),
                                            set_subtitle: Some(gettext("Save page ranges").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Split PDF")) {
                                            set_icon_name: Some("edit-cut-symbolic"),
                                            set_subtitle: Some(gettext("Create separate files").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Compress PDF")) {
                                            set_icon_name: Some("package-x-generic-symbolic"),
                                            set_subtitle: Some(gettext("Reduce file size").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Add Watermark")) {
                                            set_icon_name: Some("insert-image-symbolic"),
                                            set_subtitle: Some(gettext("Overlay an image").as_str()),
                                        },

                                        append = adw::SidebarItem::new(&gettext("Edit Metadata")) {
                                            set_icon_name: Some("document-properties-symbolic"),
                                            set_subtitle: Some(gettext("Update document details").as_str()),
                                        },
                                    }
                                }
                            }
                    },

                #[wrap(Some)]
                set_content =
                    &adw::NavigationPage {
                        set_title: &gettext("Content"),

                        #[wrap(Some)]
                        set_child =
                            &gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,

                                adw::HeaderBar {
                                    pack_end = &gtk::MenuButton {
                                        set_icon_name: "open-menu-symbolic",
                                        set_menu_model: Some(&primary_menu),
                                    }
                                },

                                gtk::Label {
                                    set_label: &gettext("Hello world!"),
                                    add_css_class: "title-header",
                                    set_vexpand: true,
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
        let model = Self {};
        let widgets = view_output!();

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
            AppMsg::Quit => main_application().quit(),
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        widgets.save_window_size().unwrap();
    }
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
