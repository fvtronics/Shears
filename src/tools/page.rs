use relm4::{ComponentParts, ComponentSender, SimpleComponent, adw, gtk};

use gettextrs::gettext;
use gtk::gio;
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

                connect_clicked[tool = model.tool] => move |button| {
                    open_pdf_dialog(tool, button);
                },
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

fn open_pdf_dialog(tool: Tool, button: &gtk::Button) {
    let dialog = pdf_dialog(tool);
    let parent = button.root().and_downcast::<gtk::Window>();

    if matches!(tool, Tool::Merge) {
        dialog.open_multiple(parent.as_ref(), None::<&gio::Cancellable>, |_| {});
    } else {
        dialog.open(parent.as_ref(), None::<&gio::Cancellable>, |_| {});
    }
}

fn pdf_dialog(tool: Tool) -> gtk::FileDialog {
    let pdf_filter = gtk::FileFilter::new();
    pdf_filter.set_name(Some(&gettext("PDF Documents")));
    pdf_filter.add_mime_type("application/pdf");
    pdf_filter.add_suffix("pdf");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&pdf_filter);

    gtk::FileDialog::builder()
        .title(tool.action_label())
        .accept_label(tool.action_label())
        .modal(true)
        .filters(&filters)
        .build()
}
