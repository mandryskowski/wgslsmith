pub mod analyzer;
pub mod context;
pub mod types;

pub use analyzer::TaintAnalyzer;
pub use context::{Metrics, TaintContext};
pub use types::TaintSet;

use ast::Module;
use std::collections::HashMap;

/// Extracts a deterministic origin ID based on the hex suffix produced by `fuse` (e.g. `_a1b2c3d4`).
/// Base shader variables won't have this matching pattern and default to `0`.
pub fn extract_origin(name: &str, suffix_map: &mut HashMap<String, u32>) -> u32 {
    if let Some(idx) = name.rfind('_') {
        let suffix = &name[idx..];
        if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            let id = suffix_map.len() as u32 + 1; // Base is 0
            return *suffix_map.entry(suffix.to_string()).or_insert(id);
        }
    }
    0 // Base shader
}

/// Runs data and control flow taint analysis on a given AST module to determine
/// the degree of data-dependency mingling introduced by skeletal program enumeration (SPE).
pub fn analyze(module: &Module) -> Metrics {
    let mut ctx = TaintContext::default();
    let mut suffix_map = HashMap::new();

    // Seed global variables with origins mapped by their fuse suffix
    for var in &module.vars {
        let origin = extract_origin(&var.name, &mut suffix_map);
        ctx.globals
            .insert(var.name.clone(), TaintSet::single(origin));
    }

    // Seed function inputs of the entry points (e.g., main)
    for func in &module.functions {
        let is_entry = func
            .attrs
            .iter()
            .any(|a| matches!(a, ast::FnAttr::Stage(_)));
        if is_entry {
            for input in &func.inputs {
                let origin = extract_origin(&input.name, &mut suffix_map);
                ctx.globals
                    .insert(input.name.clone(), TaintSet::single(origin));
            }
        }
    }

    let mut analyzer = TaintAnalyzer::new(&mut ctx);
    analyzer.analyze_module(module);

    ctx.metrics
}
