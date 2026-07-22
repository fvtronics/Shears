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
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Merge { inputs, output } => {
            let mut merge_inputs = Vec::new();
            for input in inputs {
                merge_inputs.push((MergeInput::File(input, None), 0));
            }

            let options = MergeOptions::default();

            if let Err(e) = merge_files(&merge_inputs, &output, &options) {
                eprintln!("Error merging files: {:?}", e);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
