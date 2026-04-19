use clap::{Parser, Subcommand};
use harness_types::ConfigId;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct Options {
    #[clap(long, action, default_value = "false")]
    pub skip_original: bool,

    #[clap(subcommand)]
    pub cmd: SpeCommand,
}

#[derive(Subcommand, Debug)]
pub enum SpeCommand {
    /// Enumerate and test permutations for a single shader
    Enumerate(EnumerateOptions),
    /// Process a directory of shaders
    ProcessDir(DirOptions),
    /// Fuse shaders from a directory and test permutations
    Fuse(DirOptions),
}

#[derive(Parser, Debug)]
pub struct EnumerateOptions {
    /// Path to the WGSL shader
    pub shader_path: PathBuf,

    /// Run/print a specific enumeration of a shader
    #[clap(short = 'i', long)]
    pub index: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct DirOptions {
    /// Directory to scan for WGSL shaders
    pub directory: PathBuf,

    #[clap(long)]
    pub start_index: Option<usize>,

    /// Append to an existing directory's logs and resume its stats
    #[clap(long)]
    pub append_dir: Option<PathBuf>,

    #[clap(long, action, default_value = "false")]
    pub log_to_file: bool,

    /// Control parallelism passed to run
    #[clap(short = 'j', long)]
    pub parallelism: Option<usize>,

    #[clap(long, action, default_value = "false")]
    pub use_daemon: bool,

    /// Configurations to run (e.g., wgpu:dx12:5592)
    #[clap(short, long = "config", action)]
    pub configs: Vec<ConfigId>,

    /// Address of harness server.
    #[clap(short, long, action)]
    pub server: Option<String>,

    #[clap(long, action, default_value = "false")]
    pub msl_validate: bool,

    #[clap(long, action, default_value = "false")]
    pub skip_ext_filter: bool,

    /// File containing passed shaders to skip preprocessing
    #[clap(long)]
    pub passed_shaders: Option<PathBuf>,

    /// File containing regexes to ignore crashes
    #[clap(long)]
    pub ignore_file: Option<PathBuf>,
}
