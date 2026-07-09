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
use relm4::gtk;

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
        .initial_name(gettext("output.pdf"))
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

fn parse_page_number(s: &str, max_pages: u32) -> Result<u32, String> {
    let p = s
        .trim()
        .parse::<u32>()
        .map_err(|_| gettext("Invalid input"))?;
    if p == 0 {
        return Err(gettext("Invalid input"));
    }
    if p > max_pages {
        return Err(gettext("Contains out of range pages (Max: {max})")
            .replace("{max}", &max_pages.to_string()));
    }
    Ok(p)
}

pub(super) fn validate_specific_pages(input: &str, max_pages: u32) -> Result<Vec<u32>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err(gettext("Please specify pages"));
    }

    let mut pages = Vec::new();

    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if part.contains('-') {
            return Err(gettext("Ranges are not supported for splitting"));
        }

        pages.push(parse_page_number(part, max_pages)?);
    }

    if pages.is_empty() {
        return Err(gettext("Please specify pages"));
    }

    pages.sort_unstable();
    pages.dedup();

    Ok(pages)
}

pub(super) fn validate_page_ranges(input: &str, max_pages: u32) -> Result<Vec<u32>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut pages = Vec::new();

    for part in input.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }

        if let Some((start_str, end_str)) = part.split_once('-') {
            let start = parse_page_number(start_str, max_pages)?;
            let end = parse_page_number(end_str, max_pages)?;
            if start > end {
                return Err(gettext("Invalid page range: {start}-{end}")
                    .replace("{start}", &start.to_string())
                    .replace("{end}", &end.to_string()));
            }
            for p in start..=end {
                pages.push(p);
            }
        } else {
            pages.push(parse_page_number(part, max_pages)?);
        }
    }

    pages.sort_unstable();
    pages.dedup();

    Ok(pages)
}

pub(super) fn format_page_ranges(pages: &[u32]) -> String {
    if pages.is_empty() {
        return String::new();
    }

    let mut result = Vec::new();
    let mut start = pages[0];
    let mut prev = pages[0];

    for &page in &pages[1..] {
        if page == prev + 1 {
            prev = page;
        } else {
            if start == prev {
                result.push(start.to_string());
            } else {
                result.push(format!("{}-{}", start, prev));
            }
            start = page;
            prev = page;
        }
    }

    if start == prev {
        result.push(start.to_string());
    } else {
        result.push(format!("{}-{}", start, prev));
    }

    result.join(",")
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

    #[test]
    fn validate_specific_pages_success_cases() {
        assert_eq!(
            validate_specific_pages("3, 1, 5", 10).unwrap(),
            vec![1, 3, 5]
        );
        assert_eq!(validate_specific_pages("2, 2, 3", 5).unwrap(), vec![2, 3]);
        assert_eq!(validate_specific_pages("1, 2,", 5).unwrap(), vec![1, 2]);
    }

    #[test]
    fn validate_specific_pages_error_cases() {
        assert!(validate_specific_pages("1-3", 5).is_err());
        assert!(validate_specific_pages("10", 5).is_err());
        assert!(validate_specific_pages("0", 5).is_err());
        assert!(validate_specific_pages("abc", 5).is_err());
        assert!(validate_specific_pages("", 5).is_err());
        assert!(validate_specific_pages("   ", 5).is_err());
    }

    #[test]
    fn validate_page_ranges_success_cases() {
        assert_eq!(validate_page_ranges("3", 10).unwrap(), vec![3]);
        assert_eq!(validate_page_ranges("2-5", 10).unwrap(), vec![2, 3, 4, 5]);
        assert_eq!(
            validate_page_ranges("1, 3-5, 8", 10).unwrap(),
            vec![1, 3, 4, 5, 8]
        );
        assert_eq!(
            validate_page_ranges("1-3, 2-4", 10).unwrap(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(validate_page_ranges("3-3", 10).unwrap(), vec![3]);
        assert!(validate_page_ranges("", 10).unwrap().is_empty());
    }

    #[test]
    fn validate_page_ranges_error_cases() {
        assert!(validate_page_ranges("5-2", 10).is_err());
        assert!(validate_page_ranges("1-20", 10).is_err());
    }

    #[test]
    fn format_page_ranges_output() {
        assert_eq!(format_page_ranges(&[]), "");
        assert_eq!(format_page_ranges(&[4]), "4");
        assert_eq!(format_page_ranges(&[5, 6]), "5-6");
        assert_eq!(format_page_ranges(&[1, 2, 3, 4, 5]), "1-5");
        assert_eq!(format_page_ranges(&[1, 3, 5]), "1,3,5");
        assert_eq!(format_page_ranges(&[1, 2, 3, 7, 8, 12]), "1-3,7-8,12");
    }
}
