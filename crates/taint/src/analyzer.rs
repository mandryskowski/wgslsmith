use crate::context::TaintContext;
use crate::types::TaintSet;
use ast::*;

use std::collections::{HashMap, HashSet};

pub struct TaintAnalyzer<'a> {
    pub ctx: &'a mut TaintContext,
    pub current_shader_id: u32,
    pub shader_names: HashMap<String, u32>,
    pub next_shader_id: u32,
    pub var_decl_shader: HashMap<String, u32>,
    pub fn_arg_taints: HashMap<String, Vec<TaintSet>>,
}

impl<'a> TaintAnalyzer<'a> {
    pub fn new(ctx: &'a mut TaintContext) -> Self {
        let mut shader_names = HashMap::new();
        shader_names.insert("base".to_string(), 0);
        Self {
            ctx,
            current_shader_id: 0,
            shader_names,
            next_shader_id: 1,
            var_decl_shader: HashMap::new(),
            fn_arg_taints: HashMap::new(),
        }
    }

    pub fn analyze_module(&mut self, module: &Module) {
        for func in &module.functions {
            if let Some(idx) = func.name.rfind('_') {
                let suffix = &func.name[idx..];
                if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    let hex_suffix = &suffix[1..];
                    self.shader_names
                        .entry(hex_suffix.to_string())
                        .or_insert_with(|| {
                            let new_id = self.next_shader_id;
                            self.next_shader_id += 1;
                            new_id
                        });
                }
            }
        }

        let mut register_global = |name: &str| {
            let mut origin = 0;
            if let Some(idx) = name.rfind('_') {
                let suffix = &name[idx..];
                if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    let hex_suffix = &suffix[1..];
                    let id = *self
                        .shader_names
                        .entry(hex_suffix.to_string())
                        .or_insert_with(|| {
                            let new_id = self.next_shader_id;
                            self.next_shader_id += 1;
                            new_id
                        });
                    origin = id;
                }
            }
            self.var_decl_shader.insert(name.to_string(), origin);
            self.ctx
                .globals
                .insert(name.to_string(), TaintSet::single(origin));
        };

        for var in &module.vars {
            register_global(&var.name);
        }
        for c in &module.consts {
            register_global(&c.ident);
        }
        for o in &module.overrides {
            register_global(&o.name);
        }

        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();

        for func in &module.functions {
            in_degree.insert(func.name.clone(), 0);
            adj.insert(func.name.clone(), Vec::new());
        }

        for func in &module.functions {
            let mut called = HashSet::new();
            for stmt in &func.body {
                Self::extract_calls_stmt(stmt, &mut called);
            }
            for c in called {
                if let Some(list) = adj.get_mut(&func.name) {
                    list.push(c.clone());
                }
                if let Some(deg) = in_degree.get_mut(&c) {
                    *deg += 1;
                }
            }
        }

        let mut queue = Vec::new();
        for (name, deg) in &in_degree {
            if *deg == 0 {
                queue.push(name.clone());
            }
        }

        let mut order = Vec::new();
        while let Some(name) = queue.pop() {
            order.push(name.clone());
            if let Some(list) = adj.get(&name) {
                for c in list {
                    if let Some(deg) = in_degree.get_mut(c) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(c.clone());
                        }
                    }
                }
            }
        }

        let mut visited_funcs = HashSet::new();
        for name in &order {
            visited_funcs.insert(name.clone());
        }
        for func in &module.functions {
            if !visited_funcs.contains(&func.name) {
                order.push(func.name.clone());
            }
        }

        for name in order {
            if let Some(func) = module.functions.iter().find(|f| f.name == name) {
                let mut func_origin = 0;
                if let Some(idx) = func.name.rfind('_') {
                    let suffix = &func.name[idx..];
                    if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                        let hex_suffix = &suffix[1..];
                        if let Some(&id) = self.shader_names.get(hex_suffix) {
                            func_origin = id;
                        }
                    }
                }
                self.current_shader_id = func_origin;
                self.visit_func(func);
            }
        }
    }

    fn extract_calls_expr(expr: &ExprNode, calls: &mut HashSet<String>) {
        match &expr.expr {
            Expr::FnCall(c) => {
                calls.insert(c.ident.clone());
                for arg in &c.args {
                    Self::extract_calls_expr(arg, calls);
                }
            }
            Expr::TypeCons(t) => {
                for arg in &t.args {
                    Self::extract_calls_expr(arg, calls);
                }
            }
            Expr::Postfix(p) => {
                Self::extract_calls_expr(&p.inner, calls);
                if let Postfix::Index(idx) = &p.postfix {
                    Self::extract_calls_expr(idx, calls);
                }
            }
            Expr::UnOp(u) => Self::extract_calls_expr(&u.inner, calls),
            Expr::BinOp(b) => {
                Self::extract_calls_expr(&b.left, calls);
                Self::extract_calls_expr(&b.right, calls);
            }
            Expr::Lit(_) | Expr::Var(_) => {}
        }
    }

    fn extract_calls_lhs(lhs: &LhsExprNode, calls: &mut HashSet<String>) {
        match &lhs.expr {
            LhsExpr::Postfix(inner, p) => {
                Self::extract_calls_lhs(inner, calls);
                if let Postfix::Index(idx) = p {
                    Self::extract_calls_expr(idx, calls);
                }
            }
            LhsExpr::Deref(inner) => Self::extract_calls_lhs(inner, calls),
            LhsExpr::AddressOf(inner) => Self::extract_calls_lhs(inner, calls),
            LhsExpr::Ident(_) => {}
        }
    }

    fn extract_calls_else(else_block: &Else, calls: &mut HashSet<String>) {
        match else_block {
            Else::If(s) => {
                Self::extract_calls_expr(&s.condition, calls);
                for bs in &s.body {
                    Self::extract_calls_stmt(bs, calls);
                }
                if let Some(e) = &s.else_ {
                    Self::extract_calls_else(e, calls);
                }
            }
            Else::Else(stmts) => {
                for s in stmts {
                    Self::extract_calls_stmt(s, calls);
                }
            }
        }
    }

    fn extract_calls_stmt(stmt: &Statement, calls: &mut HashSet<String>) {
        match stmt {
            Statement::LetDecl(s) => Self::extract_calls_expr(&s.initializer, calls),
            Statement::VarDecl(s) => {
                if let Some(init) = &s.initializer {
                    Self::extract_calls_expr(init, calls);
                }
            }
            Statement::ConstDecl(s) => Self::extract_calls_expr(&s.initializer, calls),
            Statement::Assignment(s) => {
                Self::extract_calls_expr(&s.rhs, calls);
                if let AssignmentLhs::Expr(lhs) = &s.lhs {
                    Self::extract_calls_lhs(lhs, calls);
                }
            }
            Statement::Compound(stmts) => {
                for s in stmts {
                    Self::extract_calls_stmt(s, calls);
                }
            }
            Statement::If(s) => {
                Self::extract_calls_expr(&s.condition, calls);
                for bs in &s.body {
                    Self::extract_calls_stmt(bs, calls);
                }
                if let Some(else_block) = &s.else_ {
                    Self::extract_calls_else(else_block, calls);
                }
            }
            Statement::Loop(s) => {
                for bs in &s.body {
                    Self::extract_calls_stmt(bs, calls);
                }
                if let Some(cont) = &s.continuing {
                    for cs in &cont.stmts {
                        Self::extract_calls_stmt(cs, calls);
                    }
                    if let Some(br) = &cont.break_if {
                        Self::extract_calls_expr(br, calls);
                    }
                }
            }
            Statement::While(s) => {
                Self::extract_calls_expr(&s.condition, calls);
                for bs in &s.body {
                    Self::extract_calls_stmt(bs, calls);
                }
            }
            Statement::ForLoop(s) => {
                if let Some(init) = &s.header.init {
                    match init {
                        ForLoopInit::VarDecl(d) => {
                            if let Some(init) = &d.initializer {
                                Self::extract_calls_expr(init, calls);
                            }
                        }
                        ForLoopInit::LetDecl(d) => Self::extract_calls_expr(&d.initializer, calls),
                        ForLoopInit::ConstDecl(d) => {
                            Self::extract_calls_expr(&d.initializer, calls)
                        }
                        ForLoopInit::Assignment(a) => {
                            Self::extract_calls_expr(&a.rhs, calls);
                            if let AssignmentLhs::Expr(lhs) = &a.lhs {
                                Self::extract_calls_lhs(lhs, calls);
                            }
                        }
                        ForLoopInit::Increment(i) => {
                            if let AssignmentLhs::Expr(lhs) = &i.lhs {
                                Self::extract_calls_lhs(lhs, calls);
                            }
                        }
                        ForLoopInit::Decrement(d) => {
                            if let AssignmentLhs::Expr(lhs) = &d.lhs {
                                Self::extract_calls_lhs(lhs, calls);
                            }
                        }
                        ForLoopInit::Call(c) => {
                            calls.insert(c.ident.clone());
                            for arg in &c.args {
                                Self::extract_calls_expr(arg, calls);
                            }
                        }
                    }
                }
                if let Some(cond) = &s.header.condition {
                    Self::extract_calls_expr(cond, calls);
                }
                if let Some(upd) = &s.header.update {
                    match upd {
                        ForLoopUpdate::Assignment(a) => {
                            Self::extract_calls_expr(&a.rhs, calls);
                            if let AssignmentLhs::Expr(lhs) = &a.lhs {
                                Self::extract_calls_lhs(lhs, calls);
                            }
                        }
                        ForLoopUpdate::Increment(i) => {
                            if let AssignmentLhs::Expr(lhs) = &i.lhs {
                                Self::extract_calls_lhs(lhs, calls);
                            }
                        }
                        ForLoopUpdate::Decrement(d) => {
                            if let AssignmentLhs::Expr(lhs) = &d.lhs {
                                Self::extract_calls_lhs(lhs, calls);
                            }
                        }
                        ForLoopUpdate::Call(c) => {
                            calls.insert(c.ident.clone());
                            for arg in &c.args {
                                Self::extract_calls_expr(arg, calls);
                            }
                        }
                    }
                }
                for bs in &s.body {
                    Self::extract_calls_stmt(bs, calls);
                }
            }
            Statement::Switch(s) => {
                Self::extract_calls_expr(&s.selector, calls);
                for case in &s.cases {
                    for bs in &case.body {
                        Self::extract_calls_stmt(bs, calls);
                    }
                }
                for bs in &s.default {
                    Self::extract_calls_stmt(bs, calls);
                }
            }
            Statement::FnCall(s) => {
                calls.insert(s.ident.clone());
                for arg in &s.args {
                    Self::extract_calls_expr(arg, calls);
                }
            }
            Statement::Increment(s) => {
                if let AssignmentLhs::Expr(lhs) = &s.lhs {
                    Self::extract_calls_lhs(lhs, calls);
                }
            }
            Statement::Decrement(s) => {
                if let AssignmentLhs::Expr(lhs) = &s.lhs {
                    Self::extract_calls_lhs(lhs, calls);
                }
            }
            Statement::Return(s) => {
                if let Some(expr) = &s.value {
                    Self::extract_calls_expr(expr, calls);
                }
            }
            Statement::ContextMarker(_)
            | Statement::Discard(_)
            | Statement::ConstAssert(_)
            | Statement::Break
            | Statement::Continue
            | Statement::Fallthrough => {}
        }
    }

    pub fn record_fn_call(&mut self, ident: &str, args: &[ExprNode]) -> Vec<TaintSet> {
        let mut arg_taints = Vec::new();
        for arg in args {
            arg_taints.push(self.eval_expr(arg));
        }
        if let Some(existing) = self.fn_arg_taints.get_mut(ident) {
            for (e, a) in existing.iter_mut().zip(arg_taints.iter()) {
                *e = e.union(a);
            }
        } else {
            self.fn_arg_taints
                .insert(ident.to_string(), arg_taints.clone());
        }
        arg_taints
    }

    pub fn assign_var(
        &mut self,
        ident: &str,
        mut rhs_taint: TaintSet,
        is_decl: bool,
        is_strong: bool,
    ) {
        self.ctx.metrics.total_assignments += 1;

        let decl_shader = if is_decl {
            self.var_decl_shader
                .insert(ident.to_string(), self.current_shader_id);
            self.current_shader_id
        } else {
            self.var_decl_shader
                .get(ident)
                .copied()
                .unwrap_or(self.current_shader_id)
        };

        if rhs_taint.has_foreign(self.current_shader_id)
            || (!is_decl && decl_shader != self.current_shader_id)
        {
            self.ctx.metrics.cross_shader_assignments += 1;
        }

        rhs_taint = rhs_taint.union(&TaintSet::single(self.current_shader_id));

        if is_decl {
            self.ctx.insert_var(ident.to_string(), rhs_taint);
        } else if is_strong {
            self.ctx.set_var(ident, rhs_taint);
        } else {
            let old = self.ctx.get_var(ident);
            self.ctx.set_var(ident, old.union(&rhs_taint));
        }
    }

    pub fn visit_func(&mut self, func: &FnDecl) {
        self.ctx.enter_scope();
        let arg_taints = self
            .fn_arg_taints
            .get(&func.name)
            .cloned()
            .unwrap_or_default();
        for (i, input) in func.inputs.iter().enumerate() {
            let taint = arg_taints.get(i).cloned().unwrap_or_default();
            self.var_decl_shader
                .insert(input.name.clone(), self.current_shader_id);
            self.ctx.insert_var(
                input.name.clone(),
                taint.union(&TaintSet::single(self.current_shader_id)),
            );
        }

        for stmt in &func.body {
            self.visit_stmt(stmt);
        }
        self.ctx.exit_scope();
    }

    pub fn visit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ContextMarker(m) => {
                let ctx_name = m
                    .context
                    .strip_prefix("FUSED_")
                    .unwrap_or(&m.context)
                    .to_string();
                let id = *self.shader_names.entry(ctx_name).or_insert_with(|| {
                    let new_id = self.next_shader_id;
                    self.next_shader_id += 1;
                    new_id
                });
                self.current_shader_id = id;
            }
            Statement::LetDecl(s) => {
                let taint = self.eval_expr(&s.initializer).union(&self.ctx.current_cf());
                self.assign_var(&s.ident, taint, true, true);
            }
            Statement::VarDecl(s) => {
                let mut taint = self.ctx.current_cf().clone();
                if let Some(init) = &s.initializer {
                    taint = taint.union(&self.eval_expr(init));
                }
                self.assign_var(&s.ident, taint, true, true);
            }
            Statement::ConstDecl(s) => {
                let taint = self.eval_expr(&s.initializer).union(&self.ctx.current_cf());
                self.assign_var(&s.ident, taint, true, true);
            }
            Statement::Assignment(s) => {
                let rhs_taint = self.eval_expr(&s.rhs).union(&self.ctx.current_cf());

                match &s.lhs {
                    AssignmentLhs::Expr(lhs_expr) => {
                        if let Some(var) = self.get_lhs_base(lhs_expr) {
                            let is_strong = s.op == AssignmentOp::Simple
                                && matches!(lhs_expr.expr, LhsExpr::Ident(_))
                                && self.ctx.cf_stack.is_empty();
                            self.assign_var(&var, rhs_taint, false, is_strong);
                        }
                    }
                    AssignmentLhs::Phony => {
                        self.ctx.metrics.total_assignments += 1;
                        if rhs_taint.has_foreign(self.current_shader_id) {
                            self.ctx.metrics.cross_shader_assignments += 1;
                        }
                    }
                }
            }
            Statement::Compound(stmts) => {
                self.ctx.enter_scope();
                for s in stmts {
                    self.visit_stmt(s);
                }
                self.ctx.exit_scope();
            }
            Statement::If(s) => {
                let cond_taint = self.eval_expr(&s.condition);
                self.ctx.push_cf(cond_taint, self.current_shader_id);

                self.ctx.enter_scope();
                for bs in &s.body {
                    self.visit_stmt(bs);
                }
                self.ctx.exit_scope();

                if let Some(else_block) = &s.else_ {
                    self.visit_else(else_block);
                }

                self.ctx.pop_cf();
            }
            Statement::Loop(s) => {
                self.ctx.enter_scope();
                for bs in &s.body {
                    self.visit_stmt(bs);
                }
                if let Some(cont) = &s.continuing {
                    self.ctx.enter_scope();
                    for cs in &cont.stmts {
                        self.visit_stmt(cs);
                    }
                    if let Some(br) = &cont.break_if {
                        self.eval_expr(br);
                    }
                    self.ctx.exit_scope();
                }
                self.ctx.exit_scope();
            }
            Statement::While(s) => {
                let cond_taint = self.eval_expr(&s.condition);
                self.ctx.push_cf(cond_taint, self.current_shader_id);

                self.ctx.enter_scope();
                for bs in &s.body {
                    self.visit_stmt(bs);
                }
                self.ctx.exit_scope();

                self.ctx.pop_cf();
            }
            Statement::ForLoop(s) => {
                self.ctx.enter_scope();
                if let Some(init) = &s.header.init {
                    match init {
                        ForLoopInit::VarDecl(d) => {
                            let mut taint = self.ctx.current_cf().clone();
                            if let Some(init) = &d.initializer {
                                taint = taint.union(&self.eval_expr(init));
                            }
                            self.assign_var(&d.ident, taint, true, true);
                        }
                        ForLoopInit::LetDecl(d) => {
                            let taint =
                                self.eval_expr(&d.initializer).union(&self.ctx.current_cf());
                            self.assign_var(&d.ident, taint, true, true);
                        }
                        ForLoopInit::ConstDecl(d) => {
                            let taint =
                                self.eval_expr(&d.initializer).union(&self.ctx.current_cf());
                            self.assign_var(&d.ident, taint, true, true);
                        }
                        ForLoopInit::Assignment(a) => {
                            let rhs_taint = self.eval_expr(&a.rhs).union(&self.ctx.current_cf());
                            match &a.lhs {
                                AssignmentLhs::Expr(lhs_expr) => {
                                    if let Some(var) = self.get_lhs_base(lhs_expr) {
                                        let is_strong = a.op == AssignmentOp::Simple
                                            && matches!(lhs_expr.expr, LhsExpr::Ident(_))
                                            && self.ctx.cf_stack.is_empty();
                                        self.assign_var(&var, rhs_taint, false, is_strong);
                                    }
                                }
                                AssignmentLhs::Phony => {
                                    self.ctx.metrics.total_assignments += 1;
                                    if rhs_taint.has_foreign(self.current_shader_id) {
                                        self.ctx.metrics.cross_shader_assignments += 1;
                                    }
                                }
                            }
                        }
                        ForLoopInit::Increment(i) => match &i.lhs {
                            AssignmentLhs::Expr(lhs) => {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    let rhs_taint = old.union(&self.ctx.current_cf());
                                    self.assign_var(&var, rhs_taint, false, false);
                                }
                            }
                            AssignmentLhs::Phony => {
                                self.ctx.metrics.total_assignments += 1;
                                if self.ctx.current_cf().has_foreign(self.current_shader_id) {
                                    self.ctx.metrics.cross_shader_assignments += 1;
                                }
                            }
                        },
                        ForLoopInit::Decrement(d) => match &d.lhs {
                            AssignmentLhs::Expr(lhs) => {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    let rhs_taint = old.union(&self.ctx.current_cf());
                                    self.assign_var(&var, rhs_taint, false, false);
                                }
                            }
                            AssignmentLhs::Phony => {
                                self.ctx.metrics.total_assignments += 1;
                                if self.ctx.current_cf().has_foreign(self.current_shader_id) {
                                    self.ctx.metrics.cross_shader_assignments += 1;
                                }
                            }
                        },
                        ForLoopInit::Call(c) => {
                            self.record_fn_call(&c.ident, &c.args);
                        }
                    }
                }

                let mut cond_taint = TaintSet::new();
                if let Some(cond) = &s.header.condition {
                    cond_taint = self.eval_expr(cond);
                }
                self.ctx.push_cf(cond_taint, self.current_shader_id);

                if let Some(upd) = &s.header.update {
                    match upd {
                        ForLoopUpdate::Assignment(a) => {
                            let rhs_taint = self.eval_expr(&a.rhs).union(&self.ctx.current_cf());
                            match &a.lhs {
                                AssignmentLhs::Expr(lhs_expr) => {
                                    if let Some(var) = self.get_lhs_base(lhs_expr) {
                                        let is_strong = a.op == AssignmentOp::Simple
                                            && matches!(lhs_expr.expr, LhsExpr::Ident(_))
                                            && self.ctx.cf_stack.is_empty();
                                        self.assign_var(&var, rhs_taint, false, is_strong);
                                    }
                                }
                                AssignmentLhs::Phony => {
                                    self.ctx.metrics.total_assignments += 1;
                                    if rhs_taint.has_foreign(self.current_shader_id) {
                                        self.ctx.metrics.cross_shader_assignments += 1;
                                    }
                                }
                            }
                        }
                        ForLoopUpdate::Increment(i) => match &i.lhs {
                            AssignmentLhs::Expr(lhs) => {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    let rhs_taint = old.union(&self.ctx.current_cf());
                                    self.assign_var(&var, rhs_taint, false, false);
                                }
                            }
                            AssignmentLhs::Phony => {
                                self.ctx.metrics.total_assignments += 1;
                                if self.ctx.current_cf().has_foreign(self.current_shader_id) {
                                    self.ctx.metrics.cross_shader_assignments += 1;
                                }
                            }
                        },
                        ForLoopUpdate::Decrement(d) => match &d.lhs {
                            AssignmentLhs::Expr(lhs) => {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    let rhs_taint = old.union(&self.ctx.current_cf());
                                    self.assign_var(&var, rhs_taint, false, false);
                                }
                            }
                            AssignmentLhs::Phony => {
                                self.ctx.metrics.total_assignments += 1;
                                if self.ctx.current_cf().has_foreign(self.current_shader_id) {
                                    self.ctx.metrics.cross_shader_assignments += 1;
                                }
                            }
                        },
                        ForLoopUpdate::Call(c) => {
                            self.record_fn_call(&c.ident, &c.args);
                        }
                    }
                }

                self.ctx.enter_scope();
                for bs in &s.body {
                    self.visit_stmt(bs);
                }
                self.ctx.exit_scope();

                self.ctx.pop_cf();
                self.ctx.exit_scope();
            }
            Statement::Switch(s) => {
                let sel_taint = self.eval_expr(&s.selector);
                self.ctx.push_cf(sel_taint, self.current_shader_id);

                for case in &s.cases {
                    self.ctx.enter_scope();
                    for bs in &case.body {
                        self.visit_stmt(bs);
                    }
                    self.ctx.exit_scope();
                }

                self.ctx.enter_scope();
                for bs in &s.default {
                    self.visit_stmt(bs);
                }
                self.ctx.exit_scope();

                self.ctx.pop_cf();
            }
            Statement::FnCall(s) => {
                self.record_fn_call(&s.ident, &s.args);
            }
            Statement::Increment(s) => match &s.lhs {
                AssignmentLhs::Expr(lhs) => {
                    if let Some(var) = self.get_lhs_base(lhs) {
                        let old = self.ctx.get_var(&var);
                        let rhs_taint = old.union(&self.ctx.current_cf());
                        self.assign_var(&var, rhs_taint, false, false);
                    }
                }
                AssignmentLhs::Phony => {
                    self.ctx.metrics.total_assignments += 1;
                    if self.ctx.current_cf().has_foreign(self.current_shader_id) {
                        self.ctx.metrics.cross_shader_assignments += 1;
                    }
                }
            },
            Statement::Decrement(s) => match &s.lhs {
                AssignmentLhs::Expr(lhs) => {
                    if let Some(var) = self.get_lhs_base(lhs) {
                        let old = self.ctx.get_var(&var);
                        let rhs_taint = old.union(&self.ctx.current_cf());
                        self.assign_var(&var, rhs_taint, false, false);
                    }
                }
                AssignmentLhs::Phony => {
                    self.ctx.metrics.total_assignments += 1;
                    if self.ctx.current_cf().has_foreign(self.current_shader_id) {
                        self.ctx.metrics.cross_shader_assignments += 1;
                    }
                }
            },
            Statement::Return(s) => {
                if let Some(expr) = &s.value {
                    self.eval_expr(expr);
                }
            }
            Statement::Discard(_)
            | Statement::ConstAssert(_)
            | Statement::Break
            | Statement::Continue
            | Statement::Fallthrough => {}
        }
    }

    pub fn visit_else(&mut self, else_block: &Else) {
        match else_block {
            Else::If(s) => {
                let cond_taint = self.eval_expr(&s.condition);
                self.ctx.push_cf(cond_taint, self.current_shader_id);

                self.ctx.enter_scope();
                for bs in &s.body {
                    self.visit_stmt(bs);
                }
                self.ctx.exit_scope();

                if let Some(e) = &s.else_ {
                    self.visit_else(e);
                }

                self.ctx.pop_cf();
            }
            Else::Else(stmts) => {
                self.ctx.enter_scope();
                for s in stmts {
                    self.visit_stmt(s);
                }
                self.ctx.exit_scope();
            }
        }
    }

    pub fn eval_expr(&mut self, expr: &ExprNode) -> TaintSet {
        match &expr.expr {
            Expr::Lit(_) => TaintSet::new(),
            Expr::Var(v) => {
                let mut taint = self.ctx.get_var(&v.ident);
                if let Some(&decl_shader) = self.var_decl_shader.get(&v.ident) {
                    taint = taint.union(&TaintSet::single(decl_shader));
                }
                taint
            }
            Expr::TypeCons(t) => {
                let mut taint = TaintSet::new();
                for arg in &t.args {
                    taint = taint.union(&self.eval_expr(arg));
                }
                taint
            }
            Expr::Postfix(p) => {
                let mut taint = self.eval_expr(&p.inner);
                if let Postfix::Index(idx) = &p.postfix {
                    taint = taint.union(&self.eval_expr(idx));
                }
                taint
            }
            Expr::UnOp(u) => self.eval_expr(&u.inner),
            Expr::BinOp(b) => {
                let l = self.eval_expr(&b.left);
                let r = self.eval_expr(&b.right);
                l.union(&r)
            }
            Expr::FnCall(c) => {
                let mut taint = TaintSet::new();
                let mut func_origin = self.current_shader_id;
                if let Some(idx) = c.ident.rfind('_') {
                    let suffix = &c.ident[idx..];
                    if suffix.len() > 1 && suffix[1..].chars().all(|ch| ch.is_ascii_hexdigit()) {
                        if let Some(&id) = self.shader_names.get(&suffix[1..]) {
                            func_origin = id;
                        }
                    }
                }
                taint = taint.union(&TaintSet::single(func_origin));
                let arg_taints = self.record_fn_call(&c.ident, &c.args);
                for arg_taint in arg_taints {
                    taint = taint.union(&arg_taint);
                }
                taint
            }
        }
    }

    pub fn get_lhs_base(&self, lhs: &LhsExprNode) -> Option<String> {
        match &lhs.expr {
            LhsExpr::Ident(name) => Some(name.clone()),
            LhsExpr::Postfix(inner, _) => self.get_lhs_base(inner),
            LhsExpr::Deref(inner) => self.get_lhs_base(inner),
            LhsExpr::AddressOf(inner) => self.get_lhs_base(inner),
        }
    }
}
