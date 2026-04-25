use crate::types::TaintSet;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Metrics {
    pub total_assignments: u32,
    pub mixed_assignments: u32,
    pub total_cf_branches: u32,
    pub mixed_cf_branches: u32,
}

#[derive(Default)]
pub struct TaintContext {
    pub globals: HashMap<String, TaintSet>,
    pub scopes: Vec<HashMap<String, TaintSet>>,
    pub cf_stack: Vec<TaintSet>,
    pub metrics: Metrics,
}

impl TaintContext {
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn insert_var(&mut self, name: String, taint: TaintSet) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, taint);
        }
    }

    pub fn set_var(&mut self, name: &str, taint: TaintSet) {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), taint);
                return;
            }
        }
        self.globals.insert(name.to_string(), taint);
    }

    pub fn get_var(&self, name: &str) -> TaintSet {
        for scope in self.scopes.iter().rev() {
            if let Some(taint) = scope.get(name) {
                return taint.clone();
            }
        }
        self.globals.get(name).cloned().unwrap_or_default()
    }

    pub fn push_cf(&mut self, taint: TaintSet) {
        if taint.is_mixed() {
            self.metrics.mixed_cf_branches += 1;
        }
        self.metrics.total_cf_branches += 1;

        let combined = self.current_cf().union(&taint);
        self.cf_stack.push(combined);
    }

    pub fn pop_cf(&mut self) {
        self.cf_stack.pop();
    }

    pub fn current_cf(&self) -> TaintSet {
        self.cf_stack.last().cloned().unwrap_or_default()
    }
}
