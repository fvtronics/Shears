use clap::{Parser, Subcommand};
use shears::pdf::merge::{MergeInput, MergeOptions, merge_files};
use shears::pdf::{DivideAfter, SplitOptions, split_file};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "shears-cli")]
#[command(about = "CLI for Shears PDF tools", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
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

            if let Err(e) = merge_files(&merge_inputs, &output, &options) {
                eprintln!("Error merging files: {:?}", e);
                std::process::exit(1);
            }
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

            if let Err(e) = split_file(&(input, 0), output, &options) {
                eprintln!("Error splitting file: {:?}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
