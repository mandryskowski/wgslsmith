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

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! test_case {
        ($name:ident) => {
            test_case!($name, $name);
        };
        ($name:ident, $fn:ident) => {
            #[test]
            fn $fn() {
                const SRC: &str = include_str!(concat!("tests/", stringify!($name), ".wgsl"));
                let ast = parser::parse(SRC);
                let metrics = analyze(&ast);
                // We use assert_display_snapshot instead of debug since `Metrics` 
                // outputs a clean string representation.
                insta::assert_display_snapshot!(metrics);
            }
        };
    }

    // You can add `.wgsl` files inside `crates/taint/src/tests/` and register them here:
    // test_case!(my_shader_test);
    test_case!(simple_assignment);
    test_case!(simple_cf);
    test_case!(only_rhs);
    test_case!(global);
    test_case!(global_postfix);
    test_case!(rhs_global_in_lhs);
    test_case!(func);
    test_case!(func2);
    test_case!(func3);
}
