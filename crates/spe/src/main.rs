use clap::Parser;
use spe::Options;

fn main() {
    let options = Options::parse();
    spe::run(options);
}
