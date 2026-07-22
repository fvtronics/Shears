use clap::{Parser, Subcommand};
use std::path::PathBuf;
use shears::pdf::merge::{merge_files, MergeInput, MergeOptions};


#[derive(Parser, Debug)]
#[command(name = "shears-cli")]
#[command(about = "CLI for shears PDF tools", long_about = None)]
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
    }

    Ok(())
}
