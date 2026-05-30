use crate::{Options, ShaderStage};
use ast::{
    AssignmentLhs, AssignmentStatement, BinOpExpr, ConstAssertStatement, ConstDeclStatement,
    DataType, DecrementStatement, Else, Expr, ExprNode, FnCallExpr, FnCallStatement, FnDecl,
    ForLoopInit, ForLoopStatement, ForLoopUpdate, IfStatement, IncrementStatement,
    LetDeclStatement, LhsExpr, Lit, LoopStatement, Module, Postfix, PostfixExpr, ReturnStatement,
    ScalarType, Statement, StorageClass, SwitchStatement, TypeConsExpr, UnOpExpr, VarDeclStatement,
    VarExpr, WhileStatement,
};
use std::collections::HashSet;

pub struct ContextChecker<'a> {
    module: &'a Module,
    options: &'a Options,
    visited: HashSet<String>,
}

impl<'a> ContextChecker<'a> {
    pub fn new(module: &'a Module, options: &'a Options) -> Self {
        Self {
            module,
            options,
            visited: HashSet::new(),
        }
    }

    pub fn is_callable(&mut self, f: &FnDecl) -> bool {
        self.check_fn(f)
    }

    fn check_fn(&mut self, f: &FnDecl) -> bool {
        if self.visited.contains(&f.name) {
            return true;
        }
        self.visited.insert(f.name.clone());
        f.body.iter().all(|stmt| self.check_stmt(stmt))
    }

    fn check_stmt(&mut self, stmt: &Statement) -> bool {
        match stmt {
            Statement::LetDecl(LetDeclStatement { initializer, .. }) => {
                self.check_expr(initializer)
            }
            Statement::ConstDecl(ConstDeclStatement { initializer, .. }) => {
                self.check_expr(initializer)
            }
            Statement::VarDecl(VarDeclStatement { initializer, .. }) => initializer
                .as_ref()
                .is_none_or(|init| self.check_expr(init)),
            Statement::ConstAssert(ConstAssertStatement { condition }) => {
                self.check_expr(condition)
            }
            Statement::Discard(_) => self.options.stage == ShaderStage::Fragment,
            Statement::Assignment(AssignmentStatement { lhs, rhs, .. }) => {
                self.check_lhs(lhs) && self.check_expr(rhs)
            }
            Statement::Increment(IncrementStatement { lhs }) => self.check_lhs(lhs),
            Statement::Decrement(DecrementStatement { lhs }) => self.check_lhs(lhs),
            Statement::Compound(stmts) => stmts.iter().all(|s| self.check_stmt(s)),
            Statement::If(IfStatement {
                condition,
                body,
                else_,
            }) => {
                self.check_expr(condition)
                    && body.iter().all(|s| self.check_stmt(s))
                    && else_.as_ref().is_none_or(|e| self.check_else(e))
            }
            Statement::Return(ReturnStatement { value }) => {
                value.as_ref().is_none_or(|v| self.check_expr(v))
            }
            Statement::Loop(LoopStatement { body, continuing }) => {
                body.iter().all(|s| self.check_stmt(s))
                    && continuing.as_ref().is_none_or(|c| {
                        c.stmts.iter().all(|s| self.check_stmt(s))
                            && c.break_if.as_ref().is_none_or(|b| self.check_expr(b))
                    })
            }
            Statement::While(WhileStatement { condition, body }) => {
                self.check_expr(condition) && body.iter().all(|s| self.check_stmt(s))
            }
            Statement::Break | Statement::Continue | Statement::Fallthrough => true,
            Statement::Switch(SwitchStatement {
                selector,
                cases,
                default,
            }) => {
                self.check_expr(selector)
                    && cases.iter().all(|c| {
                        self.check_expr(&c.selector) && c.body.iter().all(|s| self.check_stmt(s))
                    })
                    && default.iter().all(|s| self.check_stmt(s))
            }
            Statement::ForLoop(ForLoopStatement { header, body }) => {
                header.init.as_ref().is_none_or(|i| self.check_for_init(i))
                    && header.condition.as_ref().is_none_or(|c| self.check_expr(c))
                    && header
                        .update
                        .as_ref()
                        .is_none_or(|u| self.check_for_update(u))
                    && body.iter().all(|s| self.check_stmt(s))
            }
            Statement::FnCall(FnCallStatement { ident, args }) => self.check_fn_call(ident, args),
        }
    }

    fn check_else(&mut self, else_: &Else) -> bool {
        match else_ {
            Else::If(stmt) => {
                self.check_expr(&stmt.condition)
                    && stmt.body.iter().all(|s| self.check_stmt(s))
                    && stmt.else_.as_ref().is_none_or(|e| self.check_else(e))
            }
            Else::Else(stmts) => stmts.iter().all(|s| self.check_stmt(s)),
        }
    }

    fn check_for_init(&mut self, init: &ForLoopInit) -> bool {
        match init {
            ForLoopInit::VarDecl(stmt) => self.check_stmt(&Statement::VarDecl(stmt.clone())),
            ForLoopInit::LetDecl(stmt) => self.check_stmt(&Statement::LetDecl(stmt.clone())),
            ForLoopInit::ConstDecl(stmt) => self.check_stmt(&Statement::ConstDecl(stmt.clone())),
            ForLoopInit::Assignment(stmt) => self.check_stmt(&Statement::Assignment(stmt.clone())),
            ForLoopInit::Increment(stmt) => self.check_stmt(&Statement::Increment(stmt.clone())),
            ForLoopInit::Decrement(stmt) => self.check_stmt(&Statement::Decrement(stmt.clone())),
            ForLoopInit::Call(stmt) => self.check_stmt(&Statement::FnCall(stmt.clone())),
        }
    }

    fn check_for_update(&mut self, update: &ForLoopUpdate) -> bool {
        match update {
            ForLoopUpdate::Assignment(stmt) => {
                self.check_stmt(&Statement::Assignment(stmt.clone()))
            }
            ForLoopUpdate::Increment(stmt) => self.check_stmt(&Statement::Increment(stmt.clone())),
            ForLoopUpdate::Decrement(stmt) => self.check_stmt(&Statement::Decrement(stmt.clone())),
            ForLoopUpdate::Call(stmt) => self.check_stmt(&Statement::FnCall(stmt.clone())),
        }
    }

    fn check_lhs(&mut self, lhs: &AssignmentLhs) -> bool {
        match lhs {
            AssignmentLhs::Phony => true,
            AssignmentLhs::Expr(expr) => self.check_lhs_expr(&expr.expr),
        }
    }

    fn check_lhs_expr(&mut self, expr: &LhsExpr) -> bool {
        match expr {
            LhsExpr::Ident(ident) => self.check_var_access(ident),
            LhsExpr::Postfix(inner, postfix) => {
                self.check_lhs_expr(&inner.expr) && self.check_postfix(postfix)
            }
            LhsExpr::Deref(inner) | LhsExpr::AddressOf(inner) => self.check_lhs_expr(&inner.expr),
        }
    }

    fn contains_f16(&self, ty: &DataType) -> bool {
        match ty {
            DataType::Scalar(ScalarType::F16) => true,
            DataType::Vector(_, ScalarType::F16) => true,
            DataType::Matrix(_, _, ScalarType::F16) => true,
            DataType::Array(inner, _) => self.contains_f16(inner),
            DataType::Struct(decl) => decl.members.iter().any(|m| self.contains_f16(&m.data_type)),
            DataType::Ptr(view) | DataType::Ref(view) => self.contains_f16(&view.inner),
            _ => false,
        }
    }

    fn check_expr(&mut self, expr: &ExprNode) -> bool {
        if !self.options.enable_f16() && self.contains_f16(&expr.data_type) {
            return false;
        }

        match &expr.expr {
            Expr::Lit(Lit::F16(_)) => self.options.enable_f16(),
            Expr::Lit(_) => true,
            Expr::TypeCons(TypeConsExpr { args, .. }) => args.iter().all(|a| self.check_expr(a)),
            Expr::Var(VarExpr { ident }) => self.check_var_access(ident),
            Expr::Postfix(PostfixExpr { inner, postfix }) => {
                self.check_expr(inner) && self.check_postfix(postfix)
            }
            Expr::UnOp(UnOpExpr { inner, .. }) => self.check_expr(inner),
            Expr::BinOp(BinOpExpr { left, right, .. }) => {
                self.check_expr(left) && self.check_expr(right)
            }
            Expr::FnCall(FnCallExpr { ident, args, .. }) => self.check_fn_call(ident, args),
        }
    }

    fn check_postfix(&mut self, postfix: &Postfix) -> bool {
        match postfix {
            Postfix::Index(expr) => self.check_expr(expr),
            Postfix::Member(_) => true,
        }
    }

    fn check_var_access(&self, ident: &str) -> bool {
        let module = self.module;
        if let Some(var) = module.vars.iter().find(|v| v.name == ident) {
            let sc = var
                .qualifier
                .as_ref()
                .map(|q| q.storage_class)
                .unwrap_or(StorageClass::Private);
            return sc != StorageClass::WorkGroup || self.options.stage == ShaderStage::Compute;
        }
        true
    }

    fn check_fn_call(&mut self, ident: &str, args: &[ExprNode]) -> bool {
        if !args.iter().all(|a| self.check_expr(a)) {
            return false;
        }

        if !self.options.enable_f16() && ident == "f16" {
            return false;
        }

        if let Ok(builtin) = ident.parse::<ast::BuiltinFn>() {
            use ast::BuiltinFn::*;
            let valid_builtin = match builtin {
                SubgroupAdd
                | SubgroupAnd
                | SubgroupExclusiveAdd
                | SubgroupInclusiveAdd
                | SubgroupAll
                | SubgroupAny
                | SubgroupBallot
                | SubgroupBroadcast
                | SubgroupBroadcastFirst
                | SubgroupElect
                | SubgroupMax
                | SubgroupMin
                | SubgroupMul
                | SubgroupExclusiveMul
                | SubgroupInclusiveMul
                | SubgroupOr
                | SubgroupShuffle
                | SubgroupShuffleDown
                | SubgroupShuffleUp
                | SubgroupShuffleXor
                | SubgroupXor
                | QuadBroadcast
                | QuadSwapDiagonal
                | QuadSwapX
                | QuadSwapY => {
                    self.options.collectives() && self.options.stage != ShaderStage::Vertex
                }
                WorkgroupBarrier | WorkgroupUniformLoad => {
                    self.options.stage == ShaderStage::Compute
                }
                TextureBarrier | StorageBarrier => self.options.stage != ShaderStage::Vertex,
                Dpdx | DpdxCoarse | DpdxFine | Dpdy | DpdyCoarse | DpdyFine | Fwidth
                | FwidthCoarse | FwidthFine => self.options.stage == ShaderStage::Fragment,
                _ => true,
            };

            if !valid_builtin {
                return false;
            }
        }

        let module = self.module;
        if let Some(f) = module.functions.iter().find(|f| f.name == ident) {
            return self.check_fn(f);
        }

        true
    }
}
