#![allow(unused)]

use frame::Frames;
use reedline_repl_rs::clap::{Parser, Subcommand};
use std::error::Error;
use std::path::PathBuf;

pub mod file;
pub mod help;

#[derive(Parser, Debug)]
#[command(about, version, disable_help_subcommand = true)]
pub struct Args {
    /// Path to a storage file
    ///
    /// The file will be created if it does not already
    /// exist. Defaults to a temporary file.
    pub file: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Noun,
}

#[derive(Debug, Subcommand)]
pub enum Noun {
    /// File information and maintenance
    #[command(subcommand)]
    File(file::Verb),
    /// Interactive exploration of commands
    Help(help::UI),
}

fn run(mut file: Frames, noun: Noun) -> Result<Option<String>, Box<dyn Error>> {
    let outro = match noun {
        Noun::File(action) => action.with(&mut file)?,
        Noun::Help(ui) => help::run(file, ui, run)?,
    };

    Ok(outro)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let file = match args.file {
        Some(file) => Frames::path(file)?,
        None => Frames::temporary()?,
    };

    let outro = run(file, args.command)?;
    Ok(println!("{}", outro.unwrap_or_default()))
}
