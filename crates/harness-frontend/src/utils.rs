use std::collections::{HashMap, HashSet};

use ast::{
    AssignmentLhs, Else, Expr, ExprNode, ForLoopInit, ForLoopUpdate, IfStatement, LhsExpr,
    LhsExprNode, Module, Postfix, Statement,
};

pub fn remove_accessed_vars(vars: &mut HashSet<String>, module: &Module) {
    let mut fn_accessed_vars = HashMap::new();
    let mut fn_calls = HashMap::new();

    for decl in &module.functions {
        let mut visitor = VarVisitor {
            accessed_globals: HashSet::new(),
            called_funcs: HashSet::new(),
            locals: vec![HashSet::new()],
        };
        visitor.visit_fn_decl(decl);
        fn_accessed_vars.insert(decl.name.clone(), visitor.accessed_globals);
        fn_calls.insert(decl.name.clone(), visitor.called_funcs);
    }

    let mut compute_entry_points = vec![];
    for decl in &module.functions {
        for attr in &decl.attrs {
            if let ast::FnAttr::Stage(ast::ShaderStage::Compute) = attr {
                compute_entry_points.push(decl.name.clone());
            }
        }
    }

    if compute_entry_points.is_empty() {
        compute_entry_points.push("main".to_owned());
    }

    let mut reachable_funcs = HashSet::new();
    let mut queue = compute_entry_points;

    while let Some(f) = queue.pop() {
        if reachable_funcs.insert(f.clone()) {
            if let Some(calls) = fn_calls.get(&f) {
                queue.extend(calls.iter().cloned());
            }
        }
    }

    let mut all_accessed_globals = HashSet::new();
    for f in reachable_funcs {
        if let Some(accessed) = fn_accessed_vars.get(&f) {
            all_accessed_globals.extend(accessed.iter().cloned());
        }
    }

    vars.retain(|var| !all_accessed_globals.contains(var));
}

struct VarVisitor {
    accessed_globals: HashSet<String>,
    called_funcs: HashSet<String>,
    locals: Vec<HashSet<String>>,
}

impl VarVisitor {
    fn is_shadowed(&self, ident: &str) -> bool {
        self.locals.iter().any(|scope| scope.contains(ident))
    }

    fn enter_scope(&mut self) {
        self.locals.push(HashSet::new());
    }

    fn exit_scope(&mut self) {
        self.locals.pop();
    }

    fn add_local(&mut self, ident: &str) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(ident.to_string());
        }
    }

    fn visit_fn_decl(&mut self, decl: &ast::FnDecl) {
        self.enter_scope();
        for param in &decl.inputs {
            self.add_local(&param.name);
        }

        self.enter_scope();
        for stmt in &decl.body {
            self.visit_stmt(stmt);
        }
        self.exit_scope();

        self.exit_scope();
    }

    fn visit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::LetDecl(s) => {
                self.visit_expr_node(&s.initializer);
                self.add_local(&s.ident);
            }
            Statement::ConstDecl(s) => {
                self.visit_expr_node(&s.initializer);
                self.add_local(&s.ident);
            }
            Statement::VarDecl(s) => {
                if let Some(init) = &s.initializer {
                    self.visit_expr_node(init);
                }
                self.add_local(&s.ident);
            }
            Statement::Assignment(s) => {
                self.visit_assignment_lhs(&s.lhs);
                self.visit_expr_node(&s.rhs);
            }
            Statement::Compound(stmts) => {
                self.enter_scope();
                for s in stmts {
                    self.visit_stmt(s);
                }
                self.exit_scope();
            }
            Statement::If(s) => {
                self.visit_if(s);
            }
            Statement::Return(s) => {
                if let Some(v) = &s.value {
                    self.visit_expr_node(v);
                }
            }
            Statement::Loop(s) => {
                self.enter_scope();
                for stmt in &s.body {
                    self.visit_stmt(stmt);
                }
                self.exit_scope();

                if let Some(c) = &s.continuing {
                    self.enter_scope();
                    for stmt in &c.stmts {
                        self.visit_stmt(stmt);
                    }
                    if let Some(e) = &c.break_if {
                        self.visit_expr_node(e);
                    }
                    self.exit_scope();
                }
            }
            Statement::While(s) => {
                self.visit_expr_node(&s.condition);
                self.enter_scope();
                for stmt in &s.body {
                    self.visit_stmt(stmt);
                }
                self.exit_scope();
            }
            Statement::Break | Statement::Continue | Statement::Fallthrough => {}
            Statement::Switch(s) => {
                self.visit_expr_node(&s.selector);
                for case in &s.cases {
                    self.visit_expr_node(&case.selector);
                    self.enter_scope();
                    for stmt in &case.body {
                        self.visit_stmt(stmt);
                    }
                    self.exit_scope();
                }
                self.enter_scope();
                for stmt in &s.default {
                    self.visit_stmt(stmt);
                }
                self.exit_scope();
            }
            Statement::ForLoop(s) => {
                self.enter_scope();

                if let Some(init) = &s.header.init {
                    match init {
                        ForLoopInit::VarDecl(v) => {
                            if let Some(i) = &v.initializer {
                                self.visit_expr_node(i);
                            }
                            self.add_local(&v.ident);
                        }
                        ForLoopInit::LetDecl(l) => {
                            self.visit_expr_node(&l.initializer);
                            self.add_local(&l.ident);
                        }
                        ForLoopInit::ConstDecl(c) => {
                            self.visit_expr_node(&c.initializer);
                            self.add_local(&c.ident);
                        }
                        ForLoopInit::Assignment(a) => {
                            self.visit_assignment_lhs(&a.lhs);
                            self.visit_expr_node(&a.rhs);
                        }
                        ForLoopInit::Increment(inc) => {
                            self.visit_assignment_lhs(&inc.lhs);
                        }
                        ForLoopInit::Decrement(dec) => {
                            self.visit_assignment_lhs(&dec.lhs);
                        }
                        ForLoopInit::Call(c) => {
                            self.called_funcs.insert(c.ident.clone());
                            for arg in &c.args {
                                self.visit_expr_node(arg);
                            }
                        }
                    }
                }

                if let Some(cond) = &s.header.condition {
                    self.visit_expr_node(cond);
                }

                if let Some(upd) = &s.header.update {
                    match upd {
                        ForLoopUpdate::Assignment(a) => {
                            self.visit_assignment_lhs(&a.lhs);
                            self.visit_expr_node(&a.rhs);
                        }
                        ForLoopUpdate::Increment(inc) => {
                            self.visit_assignment_lhs(&inc.lhs);
                        }
                        ForLoopUpdate::Decrement(dec) => {
                            self.visit_assignment_lhs(&dec.lhs);
                        }
                        ForLoopUpdate::Call(c) => {
                            self.called_funcs.insert(c.ident.clone());
                            for arg in &c.args {
                                self.visit_expr_node(arg);
                            }
                        }
                    }
                }

                self.enter_scope();
                for stmt in &s.body {
                    self.visit_stmt(stmt);
                }
                self.exit_scope();

                self.exit_scope();
            }
            Statement::FnCall(s) => {
                self.called_funcs.insert(s.ident.clone());
                for arg in &s.args {
                    self.visit_expr_node(arg);
                }
            }
            Statement::Increment(s) => {
                self.visit_assignment_lhs(&s.lhs);
            }
            Statement::Decrement(s) => {
                self.visit_assignment_lhs(&s.lhs);
            }
            Statement::ConstAssert(s) => {
                self.visit_expr_node(&s.condition);
            }
            Statement::Discard(_) => {}
        }
    }

    fn visit_if(&mut self, s: &IfStatement) {
        self.visit_expr_node(&s.condition);
        self.enter_scope();
        for stmt in &s.body {
            self.visit_stmt(stmt);
        }
        self.exit_scope();
        if let Some(e) = &s.else_ {
            self.visit_else(e);
        }
    }

    fn visit_else(&mut self, else_: &Else) {
        match else_ {
            Else::If(s) => {
                self.visit_if(s);
            }
            Else::Else(stmts) => {
                self.enter_scope();
                for s in stmts {
                    self.visit_stmt(s);
                }
                self.exit_scope();
            }
        }
    }

    fn visit_expr_node(&mut self, expr_node: &ExprNode) {
        self.visit_expr(&expr_node.expr);
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Lit(_) => {}
            Expr::TypeCons(e) => {
                for arg in &e.args {
                    self.visit_expr_node(arg);
                }
            }
            Expr::Var(v) => {
                if !self.is_shadowed(&v.ident) {
                    self.accessed_globals.insert(v.ident.clone());
                }
            }
            Expr::Postfix(e) => {
                self.visit_expr_node(&e.inner);
                self.visit_postfix(&e.postfix);
            }
            Expr::UnOp(e) => {
                self.visit_expr_node(&e.inner);
            }
            Expr::BinOp(e) => {
                self.visit_expr_node(&e.left);
                self.visit_expr_node(&e.right);
            }
            Expr::FnCall(e) => {
                self.called_funcs.insert(e.ident.clone());
                for arg in &e.args {
                    self.visit_expr_node(arg);
                }
            }
        }
    }

    fn visit_lhs_expr_node(&mut self, lhs_expr_node: &LhsExprNode) {
        self.visit_lhs_expr(&lhs_expr_node.expr);
    }

    fn visit_lhs_expr(&mut self, lhs_expr: &LhsExpr) {
        match lhs_expr {
            LhsExpr::Ident(ident) => {
                if !self.is_shadowed(ident) {
                    self.accessed_globals.insert(ident.clone());
                }
            }
            LhsExpr::Postfix(e, postfix) => {
                self.visit_lhs_expr_node(e);
                self.visit_postfix(postfix);
            }
            LhsExpr::Deref(e) => {
                self.visit_lhs_expr_node(e);
            }
            LhsExpr::AddressOf(e) => {
                self.visit_lhs_expr_node(e);
            }
        }
    }

    fn visit_assignment_lhs(&mut self, lhs: &AssignmentLhs) {
        match lhs {
            AssignmentLhs::Phony => {}
            AssignmentLhs::Expr(e) => {
                self.visit_lhs_expr_node(e);
            }
        }
    }

    fn visit_postfix(&mut self, postfix: &Postfix) {
        match postfix {
            Postfix::Index(e) => {
                self.visit_expr_node(e);
            }
            Postfix::Member(_) => {}
        }
    }
}
