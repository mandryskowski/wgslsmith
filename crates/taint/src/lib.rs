pub mod analyzer;
pub mod context;
pub mod types;

pub use analyzer::TaintAnalyzer;
pub use context::{Metrics, TaintContext};
pub use types::TaintSet;

use ast::Module;
use clap::Parser;
#[derive(Parser)]
pub struct Options {
    /// Path to a wgsl shader program (use '-' for stdin).
    #[clap(action, default_value = "-")]
    pub input: String,
}

pub fn run(options: Options) -> eyre::Result<()> {
    let source = if options.input == "-" {
        let mut s = String::new();
        use std::io::Read;
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(&options.input)?
    };
    let ast = parser::parse(&source);
    let metrics = analyze(&ast);

    println!("Taint Metrics:\n{}", metrics);

    Ok(())
}

pub fn analyze(module: &Module) -> Metrics {
    let mut ctx = TaintContext::default();
    let mut analyzer = TaintAnalyzer::new(&mut ctx);
    analyzer.analyze_module(module);

    ctx.metrics
}
