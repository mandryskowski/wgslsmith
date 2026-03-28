use clap::Parser;
use harness::cli::{self, Command};
use harness::HarnessCommand;

fn main() -> eyre::Result<()> {
    if std::env::var("NO_COLOR") == Err(std::env::VarError::NotPresent) {
        color_eyre::install()?;
    } else {
        color_eyre::config::HookBuilder::new()
            .theme(color_eyre::config::Theme::new())
            .install()?;
    }

    env_logger::init();

    cli::run(
        HarnessCommand::new(std::env::current_exe().unwrap()),
        Command::parse(),
        vec![],
    )
}
