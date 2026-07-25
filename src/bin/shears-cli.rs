use clap::{Parser, Subcommand, ValueEnum};
use shears::pdf::extract::{ExtractOptions, extract_document};
use shears::pdf::merge::{MergeInput, MergeOptions, merge_files};
use shears::pdf::util::{load_document, validate_page_ranges};
use shears::pdf::{CompressOptions, QualityLevel, compress_file};
use shears::pdf::{DivideAfter, SplitOptions, split_file};
use std::path::PathBuf;

#[derive(ValueEnum, Clone, Debug, Default)]
enum CliQualityLevel {
    Original,
    Print,
    #[default]
    Display,
    Draft,
}

impl From<CliQualityLevel> for QualityLevel {
    fn from(lvl: CliQualityLevel) -> Self {
        match lvl {
            CliQualityLevel::Original => QualityLevel::Original,
            CliQualityLevel::Print => QualityLevel::Print,
            CliQualityLevel::Display => QualityLevel::Display,
            CliQualityLevel::Draft => QualityLevel::Draft,
        }
    }
}

#[derive(Parser, Debug)]
#[command(name = "shears-cli")]
#[command(about = "CLI for Shears PDF tools", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Compress a PDF file
    Compress {
        /// Input PDF file to compress
        #[arg(required = true)]
        input: PathBuf,

        /// Output PDF file path
        #[arg(short, long, required = true)]
        output: PathBuf,

        /// Image quality level
        #[arg(long, default_value = "display")]
        quality: CliQualityLevel,

        /// Password if the PDF is encrypted
        #[arg(long)]
        password: Option<String>,

        /// Save with PDF 1.5 object streams
        #[arg(long)]
        modern_format: bool,

        /// Remove existing metadata before saving
        #[arg(long)]
        remove_metadata: bool,
    },

    /// Merge multiple PDFs into a single file
    Merge {
        /// Input PDF files to merge
        #[arg(required = true)]
        inputs: Vec<PathBuf>,

        /// Output PDF file path
        #[arg(short, long, required = true)]
        output: PathBuf,

        /// Save with PDF 1.5 object streams
        #[arg(long)]
        modern_format: bool,

        /// Remove existing metadata before saving
        #[arg(long)]
        remove_metadata: bool,

        /// Normalize page sizes to match the largest page
        #[arg(long)]
        normalize_page_size: bool,
    },

    /// Split a PDF file into multiple files
    Split {
        /// Input PDF file to split
        #[arg(required = true)]
        input: PathBuf,

        /// Output directory
        #[arg(short, long, required = true)]
        output: PathBuf,

        /// Output files prefix
        #[arg(short, long)]
        prefix: Option<String>,

        /// Split after even pages
        #[arg(long, conflicts_with_all = ["odd", "every_n", "pages"])]
        even: bool,

        /// Split after odd pages
        #[arg(long, conflicts_with_all = ["even", "every_n", "pages"])]
        odd: bool,

        /// Split after every N pages
        #[arg(long, value_name = "N", conflicts_with_all = ["even", "odd", "pages"])]
        every_n: Option<u32>,

        /// Split after specific pages (comma-separated list)
        #[arg(long, value_delimiter = ',', conflicts_with_all = ["even", "odd", "every_n"])]
        pages: Option<Vec<u32>>,

        /// Password if the PDF is encrypted
        #[arg(long)]
        password: Option<String>,

        /// Save with PDF 1.5 object streams
        #[arg(long)]
        modern_format: bool,

        /// Remove existing metadata before saving
        #[arg(long)]
        remove_metadata: bool,
    },

    /// Extract pages from a PDF file
    Extract {
        /// Input PDF file to extract from
        #[arg(required = true)]
        input: PathBuf,

        /// Output PDF file path
        #[arg(short, long, required = true)]
        output: PathBuf,

        /// Pages to extract (e.g. "1-5,8,11-13")
        #[arg(long, required = true)]
        pages: String,

        /// Password if the PDF is encrypted
        #[arg(long)]
        password: Option<String>,

        /// Save with PDF 1.5 object streams
        #[arg(long)]
        modern_format: bool,

        /// Remove existing metadata before saving
        #[arg(long)]
        remove_metadata: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Compress {
            input,
            output,
            quality,
            password,
            modern_format,
            remove_metadata,
        } => {
            let options = CompressOptions {
                image_quality: quality.into(),
                modern_pdf_format: modern_format,
                remove_metadata,
                password,
                ..Default::default()
            };

            compress_file(&input, &output, &options)?;
        }
        Commands::Merge {
            inputs,
            output,
            modern_format,
            remove_metadata,
            normalize_page_size,
        } => {
            let mut merge_inputs = Vec::new();
            for input in inputs {
                merge_inputs.push((MergeInput::File(input, None), 0));
            }

            let options = MergeOptions {
                modern_format,
                remove_metadata,
                normalize_page_size,
            };

            merge_files(&merge_inputs, &output, &options)?;
        }
        Commands::Split {
            input,
            output,
            prefix,
            even,
            odd,
            every_n,
            pages,
            password,
            modern_format,
            remove_metadata,
        } => {
            let divide_after = if even {
                DivideAfter::EvenPages
            } else if odd {
                DivideAfter::OddPages
            } else if let Some(n) = every_n {
                DivideAfter::EveryNPages(n)
            } else if let Some(pages_list) = pages {
                DivideAfter::SpecificPages(pages_list)
            } else {
                DivideAfter::EachPage
            };

            let default_prefix = input
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();

            let options = SplitOptions {
                divide_after,
                prefix: prefix.unwrap_or(default_prefix),
                password,
                modern_format,
                remove_metadata,
            };

            split_file(&input, &output, &options)?;
        }
        Commands::Extract {
            input,
            output,
            pages,
            password,
            modern_format,
            remove_metadata,
        } => {
            let doc = load_document(&input, password.as_deref())?;

            let max_pages = doc.get_pages().len() as u32;
            let parsed_pages = validate_page_ranges(&pages, max_pages)?;

            let extract_pages = parsed_pages
                .into_iter()
                .map(|p| ((p - 1) as usize, 0))
                .collect();

            let options = ExtractOptions {
                pages: extract_pages,
                modern_pdf_format: modern_format,
                remove_metadata,
                password,
            };

            extract_document(doc, &output, &options)?;
        }
    }

    Ok(())
}
