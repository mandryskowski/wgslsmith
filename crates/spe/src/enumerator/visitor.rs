use crate::enumerator::context::Context;
use crate::enumerator::types::DeclFlags;
use ast::*;

pub fn visit_module(module: &mut Module, ctx: &mut Context) {
    for decl in &mut module.consts {
        let prev = ctx.in_const_context;
        ctx.in_const_context = true;
        visit_expr_node(&mut decl.initializer, ctx, false);
        ctx.in_const_context = prev;

        let ty = decl.inferred_type().clone();
        ctx.process_decl(
            &mut decl.ident,
            &ty,
            DeclFlags {
                is_const: true,
                ..Default::default()
            },
        );
    }
    for decl in &mut module.overrides {
        if let Some(init) = &mut decl.initializer {
            let prev = ctx.in_const_context;
            ctx.in_const_context = true;
            visit_expr_node(init, ctx, false);
            ctx.in_const_context = prev;
        }

        let ty = decl.data_type.clone().unwrap_or_else(|| {
            decl.initializer.as_ref().map(|init| {
                if let ast::DataType::Ref(view) = &init.data_type {
                    view.inner.as_ref().clone()
                } else {
                    init.data_type.clone()
                }
            }).unwrap_or(ast::DataType::Scalar(ast::ScalarType::I32))
        });
        ctx.process_decl(
            &mut decl.name,
            &ty,
            DeclFlags {
                is_const: true,
                ..Default::default()
            },
        );
    }
    for decl in &mut module.vars {
        if let Some(init) = &mut decl.initializer {
            visit_expr_node(init, ctx, false);
        }

        let ty = decl.data_type.clone();
        let mut flags = DeclFlags {
            mutable: true,
            storage_class: decl.qualifier.as_ref().map(|q| q.storage_class),
            ..Default::default()
        };

        if let Some(qualifier) = &decl.qualifier {
            let access_mode = qualifier
                .access_mode
                .unwrap_or_else(|| qualifier.storage_class.default_access_mode());
            if access_mode == ast::AccessMode::Read {
                flags.mutable = false;
            }
            if qualifier.storage_class == ast::StorageClass::Storage
                && access_mode != ast::AccessMode::Read
            {
                flags.banned_from_vertex = true;
            }
            if qualifier.storage_class == ast::StorageClass::WorkGroup {
                flags.banned_from_vertex = true;
                flags.banned_from_fragment = true;
            }
        }

        if let ast::DataType::Texture(ast::TextureType::Storage { access, .. }) = ty.dereference() {
            if *access != ast::AccessMode::Read {
                flags.banned_from_vertex = true;
            }
        }

        if let Some(view) = ty.as_memory_view() {
            if view.access_mode == ast::AccessMode::Read {
                flags.mutable = false;
            }
            if view.storage_class == ast::StorageClass::Storage
                && view.access_mode != ast::AccessMode::Read
            {
                flags.banned_from_vertex = true;
            }
            if view.storage_class == ast::StorageClass::WorkGroup {
                flags.banned_from_vertex = true;
                flags.banned_from_fragment = true;
            }
        }
        ctx.process_decl(&mut decl.name, &ty, flags);
    }
    for decl in &mut module.const_asserts {
        let prev = ctx.in_const_context;
        ctx.in_const_context = true;
        visit_expr_node(&mut decl.condition, ctx, false);
        ctx.in_const_context = prev;
    }
    for func in &mut module.functions {
        visit_fn(func, ctx);
    }
}

fn visit_fn(func: &mut FnDecl, ctx: &mut Context) {
    let prev_vertex = ctx.in_vertex_stage;
    let prev_fragment = ctx.in_fragment_stage;
    if ctx.vertex_reachable_functions.contains(&func.name) {
        ctx.in_vertex_stage = true;
    }
    if ctx.fragment_reachable_functions.contains(&func.name) {
        ctx.in_fragment_stage = true;
    }
    ctx.enter_scope();
    for input in &mut func.inputs {
        let ty = input.data_type.clone();
        ctx.process_decl(&mut input.name, &ty, DeclFlags::default());
    }
    for stmt in &mut func.body {
        visit_stmt(stmt, ctx);
    }
    ctx.exit_scope();
    ctx.in_vertex_stage = prev_vertex;
    ctx.in_fragment_stage = prev_fragment;
}

fn visit_stmt(stmt: &mut Statement, ctx: &mut Context) {
    match stmt {
        Statement::LetDecl(s) => {
            visit_expr_node(&mut s.initializer, ctx, false);
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, DeclFlags::default());
        }
        Statement::VarDecl(s) => {
            if let Some(init) = &mut s.initializer {
                visit_expr_node(init, ctx, false);
            }
            let ty = s.inferred_type().clone();
            ctx.process_decl(
                &mut s.ident,
                &ty,
                DeclFlags {
                    mutable: true,
                    storage_class: Some(ast::StorageClass::Function),
                    ..Default::default()
                },
            );
        }
        Statement::ConstDecl(s) => {
            let prev = ctx.in_const_context;
            ctx.in_const_context = true;
            visit_expr_node(&mut s.initializer, ctx, false);
            ctx.in_const_context = prev;
            let ty = s.inferred_type().clone();
            ctx.process_decl(
                &mut s.ident,
                &ty,
                DeclFlags {
                    is_const: true,
                    ..Default::default()
                },
            );
        }
        Statement::Assignment(s) => {
            visit_lhs(&mut s.lhs, ctx);
            visit_expr_node(&mut s.rhs, ctx, false);
        }
        Statement::Compound(stmts) => {
            ctx.enter_scope();
            for s in stmts {
                visit_stmt(s, ctx);
            }
            ctx.exit_scope();
        }
        Statement::If(s) => {
            visit_expr_node(&mut s.condition, ctx, false);
            ctx.enter_scope();
            for bs in &mut s.body {
                visit_stmt(bs, ctx);
            }
            ctx.exit_scope();
            if let Some(else_block) = &mut s.else_ {
                match else_block.as_mut() {
                    Else::If(if_stmt) => visit_if_statement(if_stmt, ctx),
                    Else::Else(stmts) => {
                        ctx.enter_scope();
                        for s in stmts {
                            visit_stmt(s, ctx);
                        }
                        ctx.exit_scope();
                    }
                }
            }
        }
        Statement::Return(s) => {
            if let Some(val) = &mut s.value {
                visit_expr_node(val, ctx, false);
            }
        }
        Statement::Loop(s) => {
            ctx.enter_scope();
            for bs in &mut s.body {
                visit_stmt(bs, ctx);
            }
            if let Some(cont) = &mut s.continuing {
                ctx.enter_scope();
                for cs in &mut cont.stmts {
                    visit_stmt(cs, ctx);
                }
                if let Some(br) = &mut cont.break_if {
                    visit_expr_node(br, ctx, false);
                }
                ctx.exit_scope();
            }
            ctx.exit_scope();
        }
        Statement::While(s) => {
            visit_expr_node(&mut s.condition, ctx, false);
            ctx.enter_scope();
            for bs in &mut s.body {
                visit_stmt(bs, ctx);
            }
            ctx.exit_scope();
        }
        Statement::ForLoop(s) => {
            ctx.enter_scope();
            if let Some(init) = &mut s.header.init {
                match init {
                    ForLoopInit::VarDecl(d) => {
                        if let Some(i) = &mut d.initializer {
                            visit_expr_node(i, ctx, false);
                        }
                        let ty = d.inferred_type().clone();
                        ctx.process_decl(
                            &mut d.ident,
                            &ty,
                            DeclFlags {
                                mutable: true,
                                storage_class: Some(ast::StorageClass::Function),
                                ..Default::default()
                            },
                        );
                    }
                    ForLoopInit::LetDecl(d) => {
                        visit_expr_node(&mut d.initializer, ctx, false);
                        let ty = d.inferred_type().clone();
                        ctx.process_decl(&mut d.ident, &ty, DeclFlags::default());
                    }
                    ForLoopInit::ConstDecl(d) => {
                        let prev = ctx.in_const_context;
                        ctx.in_const_context = true;
                        visit_expr_node(&mut d.initializer, ctx, false);
                        ctx.in_const_context = prev;
                        let ty = d.inferred_type().clone();
                        ctx.process_decl(
                            &mut d.ident,
                            &ty,
                            DeclFlags {
                                is_const: true,
                                ..Default::default()
                            },
                        );
                    }
                    ForLoopInit::Assignment(a) => {
                        visit_lhs(&mut a.lhs, ctx);
                        visit_expr_node(&mut a.rhs, ctx, false);
                    }
                    ForLoopInit::Increment(i) => visit_lhs(&mut i.lhs, ctx),
                    ForLoopInit::Decrement(d) => visit_lhs(&mut d.lhs, ctx),
                    ForLoopInit::Call(c) => {
                        for arg in &mut c.args {
                            visit_expr_node(arg, ctx, false);
                        }
                    }
                }
            }
            if let Some(cond) = &mut s.header.condition {
                visit_expr_node(cond, ctx, false);
            }
            if let Some(upd) = &mut s.header.update {
                match upd {
                    ForLoopUpdate::Assignment(a) => {
                        visit_lhs(&mut a.lhs, ctx);
                        visit_expr_node(&mut a.rhs, ctx, false);
                    }
                    ForLoopUpdate::Increment(i) => visit_lhs(&mut i.lhs, ctx),
                    ForLoopUpdate::Decrement(d) => visit_lhs(&mut d.lhs, ctx),
                    ForLoopUpdate::Call(c) => {
                        for arg in &mut c.args {
                            visit_expr_node(arg, ctx, false);
                        }
                    }
                }
            }
            ctx.enter_scope();
            for bs in &mut s.body {
                visit_stmt(bs, ctx);
            }
            ctx.exit_scope();
            ctx.exit_scope();
        }
        Statement::FnCall(s) => {
            for arg in &mut s.args {
                visit_expr_node(arg, ctx, false);
            }
        }
        Statement::Increment(s) => visit_lhs(&mut s.lhs, ctx),
        Statement::Decrement(s) => visit_lhs(&mut s.lhs, ctx),
        Statement::Switch(s) => {
            visit_expr_node(&mut s.selector, ctx, false);
            for case in &mut s.cases {
                ctx.enter_scope();
                for bs in &mut case.body {
                    visit_stmt(bs, ctx);
                }
                ctx.exit_scope();
            }
            ctx.enter_scope();
            for bs in &mut s.default {
                visit_stmt(bs, ctx);
            }
            ctx.exit_scope();
        }
        Statement::ConstAssert(s) => {
            let prev = ctx.in_const_context;
            ctx.in_const_context = true;
            visit_expr_node(&mut s.condition, ctx, false);
            ctx.in_const_context = prev;
        }
        _ => {}
    }
}

fn visit_if_statement(stmt: &mut IfStatement, ctx: &mut Context) {
    visit_expr_node(&mut stmt.condition, ctx, false);
    ctx.enter_scope();
    for s in &mut stmt.body {
        visit_stmt(s, ctx);
    }
    ctx.exit_scope();
    if let Some(else_block) = &mut stmt.else_ {
        match else_block.as_mut() {
            Else::If(s) => visit_if_statement(s, ctx),
            Else::Else(stmts) => {
                ctx.enter_scope();
                for s in stmts {
                    visit_stmt(s, ctx);
                }
                ctx.exit_scope();
            }
        }
    }
}

fn visit_lhs(lhs: &mut AssignmentLhs, ctx: &mut Context) {
    if let AssignmentLhs::Expr(node) = lhs {
        visit_lhs_expr_node(node, ctx);
    }
}

fn visit_lhs_expr_node(node: &mut LhsExprNode, ctx: &mut Context) {
    match &mut node.expr {
        LhsExpr::Ident(name) => {
            let ty = node.data_type.dereference().clone();
            let storage_class = node.data_type.as_memory_view().map(|v| v.storage_class);
            ctx.process_usage(name, &ty, true, false, storage_class);
        }
        LhsExpr::Postfix(inner, postfix) => {
            visit_lhs_expr_node(inner, ctx);
            if let Postfix::Index(idx_expr) = postfix {
                visit_expr_node(idx_expr, ctx, false);
            }
        }
        LhsExpr::Deref(inner) => visit_lhs_expr_node(inner, ctx),
        LhsExpr::AddressOf(inner) => visit_lhs_expr_node(inner, ctx),
    }
}

fn visit_expr_node(node: &mut ExprNode, ctx: &mut Context, is_address_of: bool) {
    match &mut node.expr {
        Expr::Var(v) => {
            let ty = node.data_type.dereference().clone();
            let storage_class = node.data_type.as_memory_view().map(|v| v.storage_class);
            ctx.process_usage(
                &mut v.ident,
                &ty,
                is_address_of,
                is_address_of,
                storage_class,
            );
        }
        Expr::FnCall(call) => {
            for arg in &mut call.args {
                visit_expr_node(arg, ctx, false);
            }
        }
        Expr::BinOp(op) => {
            visit_expr_node(&mut op.left, ctx, false);
            visit_expr_node(&mut op.right, ctx, false);
        }
        Expr::UnOp(op) => {
            let addr_of = matches!(op.op, UnOp::AddressOf);
            visit_expr_node(&mut op.inner, ctx, is_address_of || addr_of);
        }
        Expr::Postfix(p) => {
            visit_expr_node(&mut p.inner, ctx, is_address_of);
            if let Postfix::Index(idx) = &mut p.postfix {
                visit_expr_node(idx, ctx, false);
            }
        }
        Expr::TypeCons(t) => {
            for arg in &mut t.args {
                visit_expr_node(arg, ctx, false);
            }
        }
        _ => {}
    }
}
