use std::collections::HashSet;

use ast::{
    AssignmentLhs, Else, Expr, ExprNode, ForLoopInit, ForLoopUpdate, IfStatement, LhsExpr,
    LhsExprNode, Module, Postfix, Statement,
};

pub fn remove_accessed_vars(vars: &mut HashSet<String>, module: &Module) {
    let mut visitor = VarVisitor {
        vars,
        locals: vec![HashSet::new()],
    };

    for decl in &module.functions {
        visitor.visit_fn_decl(decl);
    }
}

struct VarVisitor<'a> {
    vars: &'a mut HashSet<String>,
    locals: Vec<HashSet<String>>,
}

impl<'a> VarVisitor<'a> {
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
                    self.vars.remove(&v.ident);
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
                    self.vars.remove(ident);
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
