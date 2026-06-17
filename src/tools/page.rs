use relm4::{ComponentParts, ComponentSender, SimpleComponent, adw, gtk};

use gtk::gio;
use gtk::prelude::*;

use crate::tools::{Tool, files_from_model, pdf_dialog};

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
                    open_pdf_dialog(tool, button, sender.clone());
                },
            },
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

        ComponentParts { model, widgets }
    }

    fn update(&mut self, is_loading: Self::Input, _sender: ComponentSender<Self>) {
        self.is_loading = is_loading;
    }
}

fn open_pdf_dialog(tool: Tool, button: &gtk::Button, sender: ComponentSender<ToolPage>) {
    let dialog = pdf_dialog(tool);
    let parent = button.root().and_downcast::<gtk::Window>();

    if matches!(tool, Tool::Merge) {
        dialog.open_multiple(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
            if let Ok(files) = result {
                let _ = sender.output(files_from_model(&files));
            }
        });
    } else {
        dialog.open(parent.as_ref(), None::<&gio::Cancellable>, |_| {});
    }
}
