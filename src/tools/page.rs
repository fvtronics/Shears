use adw::prelude::*;
use relm4::{ComponentParts, ComponentSender, SimpleComponent, adw, gtk};

use gtk::gio;

use crate::tools::{Tool, open_pdf_dialog};

pub struct ToolPage {
    tool: Tool,
    is_loading: bool,
}

#[relm4::component(pub)]
impl SimpleComponent for ToolPage {
    type Init = Tool;
    type Input = bool;
    type Output = Vec<gio::File>;

    view! {
        #[root]
        adw::BreakpointBin {
            set_width_request: 150,
            set_height_request: 100,

            #[name(status_page)]
            adw::StatusPage {
                set_vexpand: true,
                set_icon_name: Some(model.tool.icon_name()),
                set_title: &model.tool.empty_title(),
                set_description: Some(&model.tool.empty_description()),

                #[wrap(Some)]
                set_child = &gtk::Button {
                    set_label: &model.tool.action_label(),
                    set_halign: gtk::Align::Center,
                    set_tooltip_text: Some(&model.tool.action_label()),
                    add_css_class: "suggested-action",
                    #[watch]
                    set_sensitive: !model.is_loading,

                    connect_clicked[sender, tool = model.tool] => move |button| {
                        let sender_clone = sender.clone();
                        open_pdf_dialog(button, tool, move |files| {
                            let _ = sender_clone.output(files);
                        });
                    },
                },
            }
        }
    }

    fn init(
        tool: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self {
            tool,
            is_loading: false,
        };
        let widgets = view_output!();

        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxHeight,
            420.0,
            adw::LengthUnit::Sp,
        );
        let bp = adw::Breakpoint::new(condition);
        bp.add_setters(&[(&widgets.status_page, "icon-name", Option::<&str>::None)]);
        root.add_breakpoint(bp);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, is_loading: Self::Input, _sender: ComponentSender<Self>) {
        self.is_loading = is_loading;
    }
}
