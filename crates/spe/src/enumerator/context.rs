use crate::enumerator::types::{DeclHole, Hole, HoleType, UsageHole};
use crate::enumerator::vertex_reachable::get_vertex_reachable_functions;
use ast::types::DataType;
use ast::Module;
use std::collections::HashSet;

pub struct Context {
    pub holes: Vec<Hole>,
    pub assignments: Option<Vec<usize>>,
    pub hole_counter: usize,
    pub scope_stack: Vec<usize>,
    pub scope_parents: Vec<usize>,
    pub next_scope_id: usize,
    pub in_const_context: bool,
    pub in_vertex_stage: bool,
    pub vertex_reachable_functions: HashSet<String>,
}

impl Context {
    pub fn new(module: Option<&Module>) -> Self {
        let vertex_reachable_functions = module
            .map(get_vertex_reachable_functions)
            .unwrap_or_default();

        Context {
            holes: vec![],
            assignments: None,
            hole_counter: 0,
            scope_stack: vec![0],
            scope_parents: vec![0],
            next_scope_id: 1,
            in_const_context: false,
            in_vertex_stage: false,
            vertex_reachable_functions,
        }
    }

    pub fn enter_scope(&mut self) {
        let current = *self.scope_stack.last().unwrap();
        if self.next_scope_id >= self.scope_parents.len() {
            self.scope_parents.resize(self.next_scope_id + 1, 0);
        }
        self.scope_parents[self.next_scope_id] = current;
        self.scope_stack.push(self.next_scope_id);
        self.next_scope_id += 1;
    }

    pub fn exit_scope(&mut self) {
        self.scope_stack.pop();
    }

    pub fn current_scope(&self) -> usize {
        *self.scope_stack.last().unwrap()
    }

    pub fn process_decl(
        &mut self,
        name: &mut String,
        data_type: &DataType,
        flags: crate::enumerator::types::DeclFlags,
    ) {
        if self.assignments.is_none() {
            self.holes.push(Hole {
                hole_type: HoleType::Decl(DeclHole {
                    mutable: flags.mutable,
                    is_const: flags.is_const,
                    banned_from_vertex: flags.banned_from_vertex,
                }),
                data_type: data_type.clone(),
                scope_id: self.current_scope(),
                original_name: name.clone(),
            });
        } else if let Some(assigns) = &self.assignments {
            if self.hole_counter < assigns.len() {
                *name = format!("v{}", assigns[self.hole_counter]);
            }
            self.hole_counter += 1;
        }
    }

    pub fn process_usage(&mut self, name: &mut String, data_type: &DataType, is_lvalue: bool) {
        if self.assignments.is_none() {
            self.holes.push(Hole {
                hole_type: HoleType::Usage(UsageHole {
                    is_lvalue,
                    requires_const: self.in_const_context,
                    in_vertex_stage: self.in_vertex_stage,
                }),
                data_type: data_type.clone(),
                scope_id: self.current_scope(),
                original_name: name.clone(),
            });
        } else if let Some(assigns) = &self.assignments {
            if self.hole_counter < assigns.len() {
                *name = format!("v{}", assigns[self.hole_counter]);
            }
            self.hole_counter += 1;
        }
    }
}
