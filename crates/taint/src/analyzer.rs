use crate::context::TaintContext;
use crate::types::TaintSet;
use ast::*;

use std::collections::HashMap;

pub struct TaintAnalyzer<'a> {
    pub ctx: &'a mut TaintContext,
    pub current_shader_id: u32,
    pub shader_names: HashMap<String, u32>,
    pub next_shader_id: u32,
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
        }
    }

    pub fn analyze_module(&mut self, module: &Module) {
        for func in &module.functions {
            if let Some(idx) = func.name.rfind('_') {
                let suffix = &func.name[idx..];
                if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    let hex_suffix = &suffix[1..];
                    self.shader_names.entry(hex_suffix.to_string()).or_insert_with(|| {
                        let new_id = self.next_shader_id;
                        self.next_shader_id += 1;
                        new_id
                    });
                }
            }
        }

        for var in &module.vars {
            let mut origin = 0;
            if let Some(idx) = var.name.rfind('_') {
                let suffix = &var.name[idx..];
                if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    if let Some(&id) = self.shader_names.get(&suffix[1..]) {
                        origin = id;
                    }
                }
            }
            self.ctx.globals.insert(var.name.clone(), TaintSet::single(origin));
        }

        for func in &module.functions {
            let mut func_origin = 0;
            if let Some(idx) = func.name.rfind('_') {
                let suffix = &func.name[idx..];
                if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                    if let Some(&id) = self.shader_names.get(&suffix[1..]) {
                        func_origin = id;
                    }
                }
            }
            self.current_shader_id = func_origin;
            self.visit_func(func);
        }
    }

    pub fn visit_func(&mut self, func: &FnDecl) {
        self.ctx.enter_scope();
        for input in &func.inputs {
            let taint = self.ctx.get_var(&input.name);
            self.ctx.insert_var(input.name.clone(), taint);
        }

        for stmt in &func.body {
            self.visit_stmt(stmt);
        }
        self.ctx.exit_scope();
    }

    pub fn visit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ContextMarker(m) => {
                let id = *self.shader_names.entry(m.context.clone()).or_insert_with(|| {
                    let new_id = self.next_shader_id;
                    self.next_shader_id += 1;
                    new_id
                });
                self.current_shader_id = id;
            }
            Statement::LetDecl(s) => {
                let mut taint = self.eval_expr(&s.initializer).union(&self.ctx.current_cf());
                taint = taint.union(&TaintSet::single(self.current_shader_id));
                self.ctx.insert_var(s.ident.clone(), taint);
            }
            Statement::VarDecl(s) => {
                let mut taint = self.ctx.current_cf().union(&TaintSet::single(self.current_shader_id));
                if let Some(init) = &s.initializer {
                    taint = taint.union(&self.eval_expr(init));
                }
                self.ctx.insert_var(s.ident.clone(), taint);
            }
            Statement::ConstDecl(s) => {
                let mut taint = self.eval_expr(&s.initializer).union(&self.ctx.current_cf());
                taint = taint.union(&TaintSet::single(self.current_shader_id));
                self.ctx.insert_var(s.ident.clone(), taint);
            }
            Statement::Assignment(s) => {
                let rhs_taint = self.eval_expr(&s.rhs).union(&self.ctx.current_cf());

                if let AssignmentLhs::Expr(lhs_expr) = &s.lhs {
                    if let Some(var) = self.get_lhs_base(lhs_expr) {
                        self.ctx.metrics.total_assignments += 1;
                        if rhs_taint.has_foreign(self.current_shader_id) {
                            self.ctx.metrics.cross_shader_assignments += 1;
                        }

                        let old_taint = self.ctx.get_var(&var);
                        let new_taint = old_taint.union(&rhs_taint);
                        self.ctx.set_var(&var, new_taint);
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
                            let mut taint = self.ctx.current_cf().union(&TaintSet::single(self.current_shader_id));
                            if let Some(init) = &d.initializer {
                                taint = taint.union(&self.eval_expr(init));
                            }
                            self.ctx.insert_var(d.ident.clone(), taint);
                        }
                        ForLoopInit::LetDecl(d) => {
                            let mut taint = self.eval_expr(&d.initializer).union(&self.ctx.current_cf());
                            taint = taint.union(&TaintSet::single(self.current_shader_id));
                            self.ctx.insert_var(d.ident.clone(), taint);
                        }
                        ForLoopInit::ConstDecl(d) => {
                            let mut taint = self.eval_expr(&d.initializer).union(&self.ctx.current_cf());
                            taint = taint.union(&TaintSet::single(self.current_shader_id));
                            self.ctx.insert_var(d.ident.clone(), taint);
                        }
                        ForLoopInit::Assignment(a) => {
                            let rhs_taint = self.eval_expr(&a.rhs).union(&self.ctx.current_cf());
                            if let AssignmentLhs::Expr(lhs_expr) = &a.lhs {
                                if let Some(var) = self.get_lhs_base(lhs_expr) {
                                    self.ctx.metrics.total_assignments += 1;
                                    if rhs_taint.has_foreign(self.current_shader_id) {
                                        self.ctx.metrics.cross_shader_assignments += 1;
                                    }
                                    let old = self.ctx.get_var(&var);
                                    self.ctx.set_var(&var, old.union(&rhs_taint));
                                }
                            }
                        }
                        ForLoopInit::Increment(i) => {
                            if let AssignmentLhs::Expr(lhs) = &i.lhs {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    self.ctx.set_var(&var, old.union(&self.ctx.current_cf()));
                                }
                            }
                        }
                        ForLoopInit::Decrement(d) => {
                            if let AssignmentLhs::Expr(lhs) = &d.lhs {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    self.ctx.set_var(&var, old.union(&self.ctx.current_cf()));
                                }
                            }
                        }
                        ForLoopInit::Call(c) => {
                            for arg in &c.args {
                                self.eval_expr(arg);
                            }
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
                            if let AssignmentLhs::Expr(lhs_expr) = &a.lhs {
                                if let Some(var) = self.get_lhs_base(lhs_expr) {
                                    self.ctx.metrics.total_assignments += 1;
                                    if rhs_taint.has_foreign(self.current_shader_id) {
                                        self.ctx.metrics.cross_shader_assignments += 1;
                                    }
                                    let old = self.ctx.get_var(&var);
                                    self.ctx.set_var(&var, old.union(&rhs_taint));
                                }
                            }
                        }
                        ForLoopUpdate::Increment(i) => {
                            if let AssignmentLhs::Expr(lhs) = &i.lhs {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    self.ctx.set_var(&var, old.union(&self.ctx.current_cf()));
                                }
                            }
                        }
                        ForLoopUpdate::Decrement(d) => {
                            if let AssignmentLhs::Expr(lhs) = &d.lhs {
                                if let Some(var) = self.get_lhs_base(lhs) {
                                    let old = self.ctx.get_var(&var);
                                    self.ctx.set_var(&var, old.union(&self.ctx.current_cf()));
                                }
                            }
                        }
                        ForLoopUpdate::Call(c) => {
                            for arg in &c.args {
                                self.eval_expr(arg);
                            }
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
                for arg in &s.args {
                    self.eval_expr(arg);
                }
            }
            Statement::Increment(s) => {
                if let AssignmentLhs::Expr(lhs) = &s.lhs {
                    if let Some(var) = self.get_lhs_base(lhs) {
                        let old = self.ctx.get_var(&var);
                        self.ctx.set_var(&var, old.union(&self.ctx.current_cf()));
                    }
                }
            }
            Statement::Decrement(s) => {
                if let AssignmentLhs::Expr(lhs) = &s.lhs {
                    if let Some(var) = self.get_lhs_base(lhs) {
                        let old = self.ctx.get_var(&var);
                        self.ctx.set_var(&var, old.union(&self.ctx.current_cf()));
                    }
                }
            }
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
                self.ctx.get_var(&v.ident)
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
                    if suffix.len() > 1 && suffix[1..].chars().all(|c| c.is_ascii_hexdigit()) {
                        if let Some(&id) = self.shader_names.get(&suffix[1..]) {
                            func_origin = id;
                        }
                    }
                }
                taint = taint.union(&TaintSet::single(func_origin));
                for arg in &c.args {
                    taint = taint.union(&self.eval_expr(arg));
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
