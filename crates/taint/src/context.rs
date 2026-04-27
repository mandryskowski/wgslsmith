use crate::types::TaintSet;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Metrics {
    pub total_assignments: u32,
    pub mixed_assignments: u32,
    pub total_cf_branches: u32,
    pub mixed_cf_branches: u32,
}

impl std::fmt::Display for Metrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mix_assign_pct = if self.total_assignments > 0 {
            (self.mixed_assignments as f64 / self.total_assignments as f64) * 100.0
        } else {
            0.0
        };
        let mix_cf_pct = if self.total_cf_branches > 0 {
            (self.mixed_cf_branches as f64 / self.total_cf_branches as f64) * 100.0
        } else {
            0.0
        };
        write!(
            f,
            "  Assignments: {}/{} ({:.1}% mixed)\n  CF Branches: {}/{} ({:.1}% mixed)",
            self.mixed_assignments,
            self.total_assignments,
            mix_assign_pct,
            self.mixed_cf_branches,
            self.total_cf_branches,
            mix_cf_pct
        )
    }
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
