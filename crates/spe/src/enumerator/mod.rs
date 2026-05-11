pub mod context;
mod reachable;
pub mod search;
pub mod types;
pub mod visitor;

use ast::Module;
use context::Context;
use search::Enumerator;
use types::{Hole, HoleType};
use visitor::visit_module;

fn get_original_assignment(holes: &[Hole], scope_parents: &[usize]) -> Vec<usize> {
    let mut name_to_id: std::collections::HashMap<(usize, String), usize> =
        std::collections::HashMap::new();
    let mut current_assignment = Vec::new();
    let mut next_available_id = 0;

    for hole in holes {
        match &hole.hole_type {
            HoleType::Decl(_) => {
                let id = next_available_id;
                next_available_id += 1;
                name_to_id.insert((hole.scope_id, hole.original_name.clone()), id);
                current_assignment.push(id);
            }
            HoleType::Usage(_) => {
                let mut cur_scope = hole.scope_id;
                let mut found_id = None;
                loop {
                    if let Some(&id) = name_to_id.get(&(cur_scope, hole.original_name.clone())) {
                        found_id = Some(id);
                        break;
                    }
                    if cur_scope == 0 {
                        break;
                    }
                    cur_scope = scope_parents[cur_scope];
                }
                if let Some(id) = found_id {
                    current_assignment.push(id);
                } else {
                    panic!(
                        "No declaration found for hole {} in scope {}",
                        hole.original_name, hole.scope_id
                    );
                }
            }
        }
    }
    current_assignment
}

pub fn get_enumerations(
    module: &Module,
    limit: Option<usize>,
) -> (usize, Vec<Vec<usize>>, Option<usize>) {
    let mut ctx = Context::new(Some(module));
    let mut analyze_module = module.clone();
    visit_module(&mut analyze_module, &mut ctx);

    let mut enumerator = Enumerator::new(&ctx, limit);
    enumerator.enumerate(&mut vec![]);

    let original_assignment = get_original_assignment(&ctx.holes, &ctx.scope_parents);

    let mut original_idx = enumerator
        .results
        .iter()
        .position(|r| r == &original_assignment);

    if original_idx.is_none() {
        enumerator.results.insert(0, original_assignment.clone());
        original_idx = Some(0);
    }

    (ctx.holes.len(), enumerator.results, original_idx)
}

pub fn apply_assignment(module: &Module, assigns: &[usize]) -> String {
    let mut case_module = module.clone();
    let mut apply_ctx = Context::new(None);
    apply_ctx.assignments = Some(assigns.to_vec());
    visit_module(&mut case_module, &mut apply_ctx);
    let mut out_str = String::new();
    ast::writer::Writer::default()
        .write_module(&mut out_str, &case_module)
        .unwrap();
    out_str
}
