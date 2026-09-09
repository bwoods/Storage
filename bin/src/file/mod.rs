use frame::Frames;
use reedline_repl_rs::clap::{ArgMatches, Args, FromArgMatches, Subcommand, ValueEnum};
use std::error::Error;

pub fn verbs(args: ArgMatches, file: &mut Frames) -> Result<Option<String>, Box<dyn Error>> {
    Ok(Verb::from_arg_matches(&args)?.with(file)?)
}

#[derive(Debug, Subcommand)]
pub enum Verb {
    /// Display disk usage statistics
    ///
    /// Displays the file system usage for each frame in the file.
    #[clap(alias = "du")]
    Info(Info),
    /// Reduce disk usage (if possible)
    #[clap(alias = "gc")]
    Compact,
    /// Page-size used for I/O
    #[clap(hide = true)]
    PageSize,
    /// Path to the file on disk
    Path,
    /// Size of the file on disk
    Size,
}

impl Verb {
    pub(crate) fn with(self, file: &mut Frames) -> Result<Option<String>, Box<dyn Error>> {
        let msg = match self {
            Verb::Compact => match file.compact()? {
                false => Some("no work to do".to_string()),
                true => None,
            },
            Verb::Info { .. } => Some(file.info()?),
            Verb::PageSize => Some(file.page_size()?.to_string()),
            Verb::Path => Some(file.file_path()?.to_string()),
            Verb::Size => Some(file.file_size()?.to_string()),
        };

        Ok(msg)
    }
}

#[derive(Args, Debug)]
pub struct Info {
    /// Selects the units to display sizes in
    #[arg(short, long, default_value = "B")]
    sizes: Units,
    /// Show the sizes based on powers of 1000 (rather than 1024)
    #[arg(long)]
    si: bool,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
pub enum Units {
    /// bytes
    #[clap(name = "B")]
    B,
    /// Kilobytes
    #[clap(name = "KB")]
    KB,
    /// Megabytes
    #[clap(name = "MB")]
    MB,
    /// Gigabytes
    #[clap(name = "GB")]
    GB,
    /// Terabytes
    #[clap(name = "TB")]
    TB,
    /// Petabytes
    #[clap(name = "PB")]
    PB,
}
