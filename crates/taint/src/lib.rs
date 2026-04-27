pub mod analyzer;
pub mod context;
pub mod types;

pub use analyzer::TaintAnalyzer;
pub use context::{Metrics, TaintContext};
pub use types::TaintSet;

use ast::Module;
use clap::Parser;
use std::collections::HashMap;

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
    let var_origins = HashMap::new();
    let metrics = analyze(&ast, &var_origins);

    println!("Taint Metrics:\n{}", metrics);

    Ok(())
}

pub fn extract_origin(name: &str, suffix_map: &mut HashMap<String, u32>) -> u32 {
    if let Some(idx) = name.rfind('_') {
        let suffix = &name[idx..];
        if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            let id = suffix_map.len() as u32 + 1;
            return *suffix_map.entry(suffix.to_string()).or_insert(id);
        }
    }
    0
}

pub fn analyze(module: &Module, var_origins: &HashMap<String, TaintSet>) -> Metrics {
    let mut ctx = TaintContext::default();
    let mut suffix_map = HashMap::new();

    let mut func_origins = HashMap::new();
    for func in &module.functions {
        let origin = extract_origin(&func.name, &mut suffix_map);
        func_origins.insert(func.name.clone(), origin);
    }

    for var in &module.vars {
        if let Some(taint) = var_origins.get(&var.name) {
            ctx.globals.insert(var.name.clone(), taint.clone());
        } else {
            let origin = extract_origin(&var.name, &mut suffix_map);
            ctx.globals
                .insert(var.name.clone(), TaintSet::single(origin));
        }
    }

    for func in &module.functions {
        let is_entry = func
            .attrs
            .iter()
            .any(|a| matches!(a, ast::FnAttr::Stage(_)));
        if is_entry {
            for input in &func.inputs {
                if let Some(taint) = var_origins.get(&input.name) {
                    ctx.globals.insert(input.name.clone(), taint.clone());
                } else {
                    let origin = extract_origin(&input.name, &mut suffix_map);
                    ctx.globals
                        .insert(input.name.clone(), TaintSet::single(origin));
                }
            }
        }
    }

    let mut analyzer = TaintAnalyzer::new(&mut ctx, var_origins, &func_origins);
    analyzer.analyze_module(module);

    ctx.metrics
}
