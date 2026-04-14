use ast::*;

pub fn get_vertex_reachable_functions(module: &Module) -> std::collections::HashSet<String> {
    let mut call_graph: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    fn visit_expr_for_calls(expr: &ExprNode, calls: &mut Vec<String>) {
        match &expr.expr {
            Expr::FnCall(c) => {
                calls.push(c.ident.clone());
                for arg in &c.args {
                    visit_expr_for_calls(arg, calls);
                }
            }
            Expr::BinOp(op) => {
                visit_expr_for_calls(&op.left, calls);
                visit_expr_for_calls(&op.right, calls);
            }
            Expr::UnOp(op) => {
                visit_expr_for_calls(&op.inner, calls);
            }
            Expr::Postfix(p) => {
                visit_expr_for_calls(&p.inner, calls);
                if let Postfix::Index(idx) = &p.postfix {
                    visit_expr_for_calls(idx, calls);
                }
            }
            Expr::TypeCons(t) => {
                for arg in &t.args {
                    visit_expr_for_calls(arg, calls);
                }
            }
            _ => {}
        }
    }

    fn visit_lhs_for_calls(lhs: &LhsExprNode, calls: &mut Vec<String>) {
        match &lhs.expr {
            LhsExpr::Ident(_) => {}
            LhsExpr::Postfix(inner, postfix) => {
                visit_lhs_for_calls(inner, calls);
                if let Postfix::Index(idx_expr) = postfix {
                    visit_expr_for_calls(idx_expr, calls);
                }
            }
            LhsExpr::Deref(inner) => visit_lhs_for_calls(inner, calls),
            LhsExpr::AddressOf(inner) => visit_lhs_for_calls(inner, calls),
        }
    }

    fn visit_stmt_for_calls(stmt: &Statement, calls: &mut Vec<String>) {
        match stmt {
            Statement::LetDecl(s) => visit_expr_for_calls(&s.initializer, calls),
            Statement::VarDecl(s) => {
                if let Some(init) = &s.initializer {
                    visit_expr_for_calls(init, calls);
                }
            }
            Statement::ConstDecl(s) => visit_expr_for_calls(&s.initializer, calls),
            Statement::Assignment(s) => {
                if let AssignmentLhs::Expr(e) = &s.lhs {
                    visit_lhs_for_calls(e, calls);
                }
                visit_expr_for_calls(&s.rhs, calls);
            }
            Statement::Compound(stmts) => {
                for s in stmts {
                    visit_stmt_for_calls(s, calls);
                }
            }
            Statement::If(s) => {
                visit_expr_for_calls(&s.condition, calls);
                for bs in &s.body {
                    visit_stmt_for_calls(bs, calls);
                }
                if let Some(else_block) = &s.else_ {
                    match else_block.as_ref() {
                        Else::If(if_stmt) => {
                            visit_stmt_for_calls(&Statement::If(if_stmt.clone()), calls)
                        }
                        Else::Else(stmts) => {
                            for s in stmts {
                                visit_stmt_for_calls(s, calls);
                            }
                        }
                    }
                }
            }
            Statement::Return(s) => {
                if let Some(val) = &s.value {
                    visit_expr_for_calls(val, calls);
                }
            }
            Statement::Loop(s) => {
                for bs in &s.body {
                    visit_stmt_for_calls(bs, calls);
                }
                if let Some(cont) = &s.continuing {
                    for cs in &cont.stmts {
                        visit_stmt_for_calls(cs, calls);
                    }
                    if let Some(br) = &cont.break_if {
                        visit_expr_for_calls(br, calls);
                    }
                }
            }
            Statement::While(s) => {
                visit_expr_for_calls(&s.condition, calls);
                for bs in &s.body {
                    visit_stmt_for_calls(bs, calls);
                }
            }
            Statement::ForLoop(s) => {
                if let Some(init) = &s.header.init {
                    match init {
                        ForLoopInit::VarDecl(d) => {
                            visit_stmt_for_calls(&Statement::VarDecl(d.clone()), calls)
                        }
                        ForLoopInit::LetDecl(d) => {
                            visit_stmt_for_calls(&Statement::LetDecl(d.clone()), calls)
                        }
                        ForLoopInit::ConstDecl(d) => {
                            visit_stmt_for_calls(&Statement::ConstDecl(d.clone()), calls)
                        }
                        ForLoopInit::Assignment(a) => {
                            visit_stmt_for_calls(&Statement::Assignment(a.clone()), calls)
                        }
                        ForLoopInit::Increment(i) => {
                            visit_stmt_for_calls(&Statement::Increment(i.clone()), calls)
                        }
                        ForLoopInit::Decrement(d) => {
                            visit_stmt_for_calls(&Statement::Decrement(d.clone()), calls)
                        }
                        ForLoopInit::Call(c) => {
                            visit_stmt_for_calls(&Statement::FnCall(c.clone()), calls)
                        }
                    }
                }
                if let Some(cond) = &s.header.condition {
                    visit_expr_for_calls(cond, calls);
                }
                if let Some(upd) = &s.header.update {
                    match upd {
                        ForLoopUpdate::Assignment(a) => {
                            visit_stmt_for_calls(&Statement::Assignment(a.clone()), calls)
                        }
                        ForLoopUpdate::Increment(i) => {
                            visit_stmt_for_calls(&Statement::Increment(i.clone()), calls)
                        }
                        ForLoopUpdate::Decrement(d) => {
                            visit_stmt_for_calls(&Statement::Decrement(d.clone()), calls)
                        }
                        ForLoopUpdate::Call(c) => {
                            visit_stmt_for_calls(&Statement::FnCall(c.clone()), calls)
                        }
                    }
                }
                for bs in &s.body {
                    visit_stmt_for_calls(bs, calls);
                }
            }
            Statement::FnCall(s) => {
                calls.push(s.ident.clone());
                for arg in &s.args {
                    visit_expr_for_calls(arg, calls);
                }
            }
            Statement::Increment(s) => {
                if let AssignmentLhs::Expr(e) = &s.lhs {
                    visit_lhs_for_calls(e, calls);
                }
            }
            Statement::Decrement(s) => {
                if let AssignmentLhs::Expr(e) = &s.lhs {
                    visit_lhs_for_calls(e, calls);
                }
            }
            Statement::Switch(s) => {
                visit_expr_for_calls(&s.selector, calls);
                for case in &s.cases {
                    for bs in &case.body {
                        visit_stmt_for_calls(bs, calls);
                    }
                }
                for bs in &s.default {
                    visit_stmt_for_calls(bs, calls);
                }
            }
            _ => {}
        }
    }

    for func in &module.functions {
        let mut calls = Vec::new();
        for stmt in &func.body {
            visit_stmt_for_calls(stmt, &mut calls);
        }
        call_graph.insert(func.name.clone(), calls);
    }

    let mut reachable = std::collections::HashSet::new();
    let mut queue = Vec::new();

    for func in &module.functions {
        if func
            .attrs
            .iter()
            .any(|a| matches!(a, FnAttr::Stage(ShaderStage::Vertex)))
        {
            reachable.insert(func.name.clone());
            queue.push(func.name.clone());
        }
    }

    while let Some(f) = queue.pop() {
        if let Some(calls) = call_graph.get(&f) {
            for target in calls {
                if reachable.insert(target.clone()) {
                    queue.push(target.clone());
                }
            }
        }
    }

    reachable
}
