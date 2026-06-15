use relm4::{ComponentParts, ComponentSender, SimpleComponent, adw, gtk};

use gtk::prelude::*;

use crate::tools::Tool;

pub struct ToolPage {
    tool: Tool,
}

#[relm4::component(pub)]
impl SimpleComponent for ToolPage {
    type Init = Tool;
    type Input = ();
    type Output = ();

    view! {
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
            },
        }
    }

    fn init(
        tool: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { tool };
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }
}
