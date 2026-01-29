use clap::StructOpt;
use generator_div::Options;

fn main() -> eyre::Result<()> {
    color_eyre::install()?;
    generator_div::run(Options::parse())
}
