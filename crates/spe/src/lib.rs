pub mod commands;
pub mod enumerator;
pub mod options;
pub mod processor;
pub mod stats;
pub mod util;
pub mod wgslsmith;

pub use options::Options;

pub fn run(options: Options) {
    stats::init();

    if let Err(e) = ctrlc::set_handler(move || {
        println!("\nProcess interrupted by user.");
        stats::print_stats();
        std::process::exit(0);
    }) {
        eprintln!("Warning: Failed to set Ctrl-C handler: {}", e);
    }

    match options.cmd {
        options::SpeCommand::Enumerate(opt) => {
            commands::enumerate::run(opt, options.skip_original);
        }
        options::SpeCommand::ProcessDir(opt) => {
            commands::process_dir::run(opt, options.skip_original);
        }
        options::SpeCommand::Fuse(opt) => {
            commands::fuse::run(opt, options.skip_original);
        }
    }
}
