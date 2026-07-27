/* tools/mod.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

pub mod compress;
pub mod extract;
pub mod merge;
pub mod metadata;
pub mod organize;
pub mod page;
pub mod split;
pub mod watermark;

use gettextrs::gettext;
use relm4::{adw, gtk};

use adw::prelude::{AlertDialogExt, AlertDialogExtManual};
use gtk::gio;
use gtk::prelude::{Cast, CastNone, FileExt, ListModelExt, WidgetExt};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ToolState {
    Empty,
    LoadingNewFile,
    Ready,
    Processing,
}

impl ToolState {
    pub fn update_loading(&mut self, is_loading: bool) {
        if is_loading {
            if *self == ToolState::Empty {
                *self = ToolState::LoadingNewFile;
            } else if *self == ToolState::Ready {
                *self = ToolState::Processing;
            }
        } else if *self == ToolState::LoadingNewFile || *self == ToolState::Processing {
            *self = ToolState::Ready;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutput {
    Loading(bool),
    Subtitle(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageOutput {
    FileActive(Option<String>),
    Loading(bool),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PreviewStatus {
    InitialPending,
    Ready,
    PasswordRequired,
    Reloading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Merge,
    Organize,
    Extract,
    Split,
    Compress,
    Watermark,
    Metadata,
}

impl Tool {
    pub fn from_index(index: u32) -> Self {
        match index {
            0 => Self::Merge,
            1 => Self::Organize,
            2 => Self::Extract,
            3 => Self::Split,
            4 => Self::Compress,
            5 => Self::Watermark,
            6 => Self::Metadata,
            _ => Self::Merge,
        }
    }

    pub fn stack_name(self) -> &'static str {
        match self {
            Tool::Merge => "merge",
            Tool::Organize => "organize",
            Tool::Extract => "extract",
            Tool::Split => "split",
            Tool::Compress => "compress",
            Tool::Watermark => "watermark",
            Tool::Metadata => "metadata",
        }
    }

    pub fn title(self) -> String {
        match self {
            Tool::Merge => gettext("Merge PDFs"),
            Tool::Organize => gettext("Organize Pages"),
            Tool::Extract => gettext("Extract Pages"),
            Tool::Split => gettext("Split PDF"),
            Tool::Compress => gettext("Compress PDF"),
            Tool::Watermark => gettext("Add Watermark"),
            Tool::Metadata => gettext("Edit Metadata"),
        }
    }

    pub fn subtitle(self) -> String {
        match self {
            Tool::Merge => gettext("No files selected"),
            Tool::Organize
            | Tool::Extract
            | Tool::Split
            | Tool::Compress
            | Tool::Watermark
            | Tool::Metadata => gettext("No file selected"),
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Tool::Merge => "view-paged-symbolic",
            Tool::Organize => "view-grid-symbolic",
            Tool::Extract => "edit-copy-symbolic",
            Tool::Split => "edit-cut-symbolic",
            Tool::Compress => "package-x-generic-symbolic",
            Tool::Watermark => "insert-image-symbolic",
            Tool::Metadata => "document-properties-symbolic",
        }
    }

    pub fn empty_title(self) -> String {
        match self {
            Tool::Merge => gettext("No PDFs Added"),
            Tool::Organize
            | Tool::Extract
            | Tool::Split
            | Tool::Compress
            | Tool::Watermark
            | Tool::Metadata => gettext("No PDF Open"),
        }
    }

    pub fn empty_description(self) -> String {
        match self {
            Tool::Merge => gettext("Add two or more PDFs to merge them"),
            Tool::Organize => gettext("Open a PDF to reorder or remove pages"),
            Tool::Extract => gettext("Open a PDF to choose pages to extract"),
            Tool::Split => gettext("Open a PDF to choose where to split it"),
            Tool::Compress => gettext("Open a PDF to save a smaller copy"),
            Tool::Watermark => gettext("Open a PDF to add an image watermark"),
            Tool::Metadata => gettext("Open a PDF to edit its metadata"),
        }
    }

    pub fn action_label(self) -> String {
        match self {
            Tool::Merge => gettext("Add PDFs"),
            Tool::Organize
            | Tool::Extract
            | Tool::Split
            | Tool::Compress
            | Tool::Watermark
            | Tool::Metadata => gettext("Open PDF"),
        }
    }

    pub fn default_output_name(self) -> String {
        match self {
            Tool::Merge => gettext("merged.pdf"),
            Tool::Split => gettext("split"),
            Tool::Organize => gettext("organized.pdf"),
            Tool::Extract => gettext("extracted.pdf"),
            Tool::Compress => gettext("compressed.pdf"),
            Tool::Watermark => gettext("watermarked.pdf"),
            Tool::Metadata => gettext("metadata.pdf"),
        }
    }
}

pub(super) fn pdf_dialog(tool: Tool) -> gtk::FileDialog {
    let pdf_filter = gtk::FileFilter::new();
    pdf_filter.set_name(Some(&gettext("PDF Documents")));
    pdf_filter.add_mime_type("application/pdf");
    pdf_filter.add_suffix("pdf");

    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&pdf_filter);

    gtk::FileDialog::builder()
        .title(tool.action_label())
        .accept_label(tool.action_label())
        .initial_name(tool.default_output_name())
        .modal(true)
        .filters(&filters)
        .build()
}

pub(super) fn files_from_model(model: &gio::ListModel) -> Vec<gio::File> {
    (0..model.n_items())
        .filter_map(|index| model.item(index))
        .filter_map(|item| item.downcast::<gio::File>().ok())
        .collect()
}

pub(super) fn open_pdf_dialog(
    button: &gtk::Button,
    tool: Tool,
    callback: impl FnOnce(Vec<gio::File>) + 'static,
) {
    let dialog = pdf_dialog(tool);
    let parent = button.root().and_downcast::<gtk::Window>();

    if matches!(tool, Tool::Merge) {
        dialog.open_multiple(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
            if let Ok(files) = result {
                callback(files_from_model(&files));
            }
        });
    } else {
        dialog.open(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                callback(vec![file]);
            }
        });
    }
}

pub(super) fn save_pdf_dialog(
    button: &gtk::Button,
    tool: Tool,
    title: &str,
    callback: impl FnOnce(gio::File) + 'static,
) {
    let dialog = pdf_dialog(tool);
    dialog.set_title(title);
    let accept_label = gettext("Save");
    dialog.set_accept_label(Some(accept_label.as_str()));
    let parent = button.root().and_downcast::<gtk::Window>();

    dialog.save(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
        if let Ok(file) = result {
            callback(file);
        }
    });
}

pub(super) fn select_folder_dialog(
    button: &gtk::Button,
    title: &str,
    callback: impl FnOnce(gio::File) + 'static,
) {
    let dialog = gtk::FileDialog::builder().title(title).modal(true).build();
    let parent = button.root().and_downcast::<gtk::Window>();

    dialog.select_folder(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
        if let Ok(file) = result {
            callback(file);
        }
    });
}

pub(super) fn confirm_dialog(
    button: &gtk::Button,
    heading: &str,
    body: &str,
    confirm_label: &str,
    confirm_appearance: adw::ResponseAppearance,
    callback: impl FnOnce() + 'static,
) {
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();

    dialog.add_response("cancel", &gettext("Cancel"));
    dialog.add_response("confirm", confirm_label);
    dialog.set_response_appearance("confirm", confirm_appearance);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let parent = button.root().and_downcast::<gtk::Window>();
    dialog.choose(
        parent.as_ref(),
        None::<&gio::Cancellable>,
        move |response| {
            if response == "confirm" {
                callback();
            }
        },
    );
}

pub(super) fn translate_page_error(err: shears::pdf::error::PageRangeError) -> String {
    use shears::pdf::error::PageRangeError;
    match err {
        PageRangeError::InvalidInput => gettext("Invalid input"),
        PageRangeError::OutOfBounds { max, .. } => {
            gettext("Contains out of range pages (Max: {max})").replace("{max}", &max.to_string())
        }
        PageRangeError::InvalidRange { start, end } => gettext("Invalid page range: {start}-{end}")
            .replace("{start}", &start.to_string())
            .replace("{end}", &end.to_string()),
        PageRangeError::RangesNotSupported => gettext("Ranges are not supported for splitting"),
        PageRangeError::Empty => gettext("Please specify pages"),
    }
}

pub(super) fn file_stem(file: &gio::File) -> String {
    file.basename()
        .and_then(|name| {
            std::path::Path::new(&name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| file.uri().to_string())
}

pub(super) fn file_name(file: &gio::File) -> String {
    file.basename()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file.uri().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_state_transitions() {
        let mut state = ToolState::Empty;
        state.update_loading(false);
        assert_eq!(state, ToolState::Empty);

        state.update_loading(true);
        assert_eq!(state, ToolState::LoadingNewFile);

        state.update_loading(false);
        assert_eq!(state, ToolState::Ready);

        state.update_loading(false);
        assert_eq!(state, ToolState::Ready);

        state.update_loading(true);
        assert_eq!(state, ToolState::Processing);

        state.update_loading(false);
        assert_eq!(state, ToolState::Ready);
    }
}
