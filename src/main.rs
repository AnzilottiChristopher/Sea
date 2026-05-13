mod analyzer_state;
mod diagnostics;
mod sea;
mod variable_info;

use clap::Parser;
use std::path::PathBuf;

use crate::sea::Sea;

#[derive(Parser, Debug)]
#[command(name = "sea")]
#[command(about = "A borrow checker for C code")]
struct Cli {
    file: PathBuf,
}

fn main() {
    let cli = Cli::parse();

    let sea = Sea::new(&cli.file);
    let diagnostics = sea.analyze(&cli.file.to_string_lossy());

    if diagnostics.is_empty() {
        println!("No issues found.");
    } else {
        for d in diagnostics {
            d.display();
        }
    }
}
