/* pdf/error.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

#[derive(Debug)]
pub enum PdfError {
    Io(std::io::Error),
    Lopdf(lopdf::Error),
    Image(image::ImageError),
    PageRange(PageRangeError),
    Other(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum PageRangeError {
    InvalidInput,
    OutOfBounds { max: u32, given: u32 },
    InvalidRange { start: u32, end: u32 },
    RangesNotSupported,
    Empty,
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {}", err),
            Self::Lopdf(err) => write!(f, "PDF error: {}", err),
            Self::Image(err) => write!(f, "Image error: {}", err),
            Self::PageRange(err) => write!(f, "Page range error: {}", err),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::fmt::Display for PageRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput => write!(f, "Invalid input"),
            Self::OutOfBounds { max, given } => {
                write!(f, "Page {} is out of bounds (Max: {})", given, max)
            }
            Self::InvalidRange { start, end } => write!(f, "Invalid page range: {}-{}", start, end),
            Self::RangesNotSupported => write!(f, "Ranges are not supported"),
            Self::Empty => write!(f, "Please specify pages"),
        }
    }
}

impl std::error::Error for PdfError {}

impl From<std::io::Error> for PdfError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<lopdf::Error> for PdfError {
    fn from(err: lopdf::Error) -> Self {
        Self::Lopdf(err)
    }
}

impl From<image::ImageError> for PdfError {
    fn from(err: image::ImageError) -> Self {
        Self::Image(err)
    }
}

impl std::error::Error for PageRangeError {}

impl From<PageRangeError> for PdfError {
    fn from(err: PageRangeError) -> Self {
        Self::PageRange(err)
    }
}
