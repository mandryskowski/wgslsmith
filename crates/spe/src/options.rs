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
    #[clap(action, default_value = "-")]
    pub shader_path: String,

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

    #[clap(long, action, default_value = "false", name = "use_daemon_flag")]
    pub use_daemon: bool,

    /// Whether to use
    #[clap(long, action, requires = "use_daemon_flag")]
    pub daemon_port: Option<u16>,

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

    /// Allow generating shaders in spe fuse.
    #[clap(long, action, default_value = "false")]
    pub allow_generate: bool,

    /// Print the generated shaders to stdout before execution
    #[clap(long, action, default_value = "false")]
    pub print: bool,

    /// Pass unstable_float to the generator
    #[clap(long, action, default_value = "false")]
    pub unstable_float: bool,

    /// Print taint analysis metrics for fused shaders
    #[clap(long, action, default_value = "false")]
    pub print_taint: bool,
}
