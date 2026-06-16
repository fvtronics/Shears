/* pdf/error.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */
use std::path::PathBuf;

#[derive(Debug)]
pub enum PdfError {
    Io(std::io::Error),
    Lopdf(lopdf::Error),
    InvalidPath(PathBuf),
    Other(String),
}

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "I/O error: {}", err),
            Self::Lopdf(err) => write!(f, "PDF error: {}", err),
            Self::InvalidPath(path) => write!(f, "Invalid file path: {:?}", path),
            Self::Other(msg) => write!(f, "{}", msg),
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
