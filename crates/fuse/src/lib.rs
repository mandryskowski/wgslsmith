use ast::types::DataType;
use ast::*;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::{BuildHasher, Hasher};
use std::rc::Rc;

pub fn fuse(module_a: Module, mut module_b: Module) -> Module {
    let hasher = RandomState::new().build_hasher();
    let hash = hasher.finish();
    let suffix = format!("_{:x}", hash as u32);

    rename_module(&mut module_b, &suffix);
    adjust_groups(&module_a, &mut module_b);
    fuse_modules(module_a, module_b)
}

fn adjust_groups(module_a: &Module, module_b: &mut Module) {
    let mut max_group_a: i32 = -1;
    for var in &module_a.vars {
        for attr in &var.attrs {
            if let GlobalVarAttr::Group(g) = attr {
                max_group_a = max_group_a.max(*g);
            }
        }
    }
    let group_offset = max_group_a + 1;

    if group_offset > 0 {
        for var in &mut module_b.vars {
            for attr in &mut var.attrs {
                if let GlobalVarAttr::Group(g) = attr {
                    *g += group_offset;
                }
            }
        }
    }
}

fn fuse_modules(mut module_a: Module, module_b: Module) -> Module {
    module_a.enables.extend(module_b.enables);
    module_a.enables.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    module_a.enables.dedup();

    module_a.requires.extend(module_b.requires);
    module_a.requires.sort_by(|a, b| a.as_ref().cmp(b.as_ref()));
    module_a.requires.dedup();

    module_a.aliases.extend(module_b.aliases);
    module_a.structs.extend(module_b.structs);
    module_a.consts.extend(module_b.consts);
    module_a.overrides.extend(module_b.overrides);
    module_a.vars.extend(module_b.vars);
    module_a.const_asserts.extend(module_b.const_asserts);

    let mut b_funcs = Vec::new();

    for f_b in module_b.functions {
        let mut b_stage = None;
        for attr in &f_b.attrs {
            if let FnAttr::Stage(stage) = attr {
                b_stage = Some(*stage);
                break;
            }
        }

        if let Some(stage) = b_stage {
            let mut matched = false;
            for f_a in &mut module_a.functions {
                let mut a_stage = None;
                for attr in &f_a.attrs {
                    if let FnAttr::Stage(s) = attr {
                        a_stage = Some(*s);
                        break;
                    }
                }

                if a_stage == Some(stage) {
                    f_a.body.extend(f_b.body.clone());
                    matched = true;
                    break;
                }
            }

            if !matched {
                b_funcs.push(f_b);
            }
        } else {
            b_funcs.push(f_b);
        }
    }

    module_a.functions.extend(b_funcs);

    module_a
}

fn rename_module(module: &mut Module, suffix: &str) {
    let mut globals = HashSet::new();

    for s in &module.structs {
        globals.insert(s.name.clone());
    }
    for a in &module.aliases {
        globals.insert(a.name.clone());
    }
    for c in &module.consts {
        globals.insert(c.ident.clone());
    }
    for o in &module.overrides {
        globals.insert(o.name.clone());
    }
    for v in &module.vars {
        globals.insert(v.name.clone());
    }
    for f in &module.functions {
        globals.insert(f.name.clone());
    }

    for decl in &mut module.structs {
        let new_name = format!("{}{}", decl.name, suffix);
        let mut new_members = vec![];
        for m in &decl.members {
            let mut new_dt = m.data_type.clone();
            rename_type(&mut new_dt, suffix, &globals);
            new_members.push(StructMember::new(m.attrs.clone(), m.name.clone(), new_dt));
        }
        *decl = StructDecl::new(new_name, new_members);
    }

    for alias in &mut module.aliases {
        alias.name.push_str(suffix);
        rename_type(&mut alias.data_type, suffix, &globals);
    }

    for c in &mut module.consts {
        c.ident.push_str(suffix);
        if let Some(ty) = &mut c.data_type {
            rename_type(ty, suffix, &globals);
        }
        rename_expr(&mut c.initializer, suffix, &globals);
    }

    for o in &mut module.overrides {
        o.name.push_str(suffix);
        if let Some(ty) = &mut o.data_type {
            rename_type(ty, suffix, &globals);
        }
        if let Some(expr) = &mut o.initializer {
            rename_expr(expr, suffix, &globals);
        }
    }

    for v in &mut module.vars {
        v.name.push_str(suffix);
        rename_type(&mut v.data_type, suffix, &globals);
        if let Some(expr) = &mut v.initializer {
            rename_expr(expr, suffix, &globals);
        }
    }

    for f in &mut module.functions {
        f.name.push_str(suffix);
        let mut scope_vars = globals.clone();
        for input in &mut f.inputs {
            scope_vars.insert(input.name.clone());
            input.name.push_str(suffix);
            rename_type(&mut input.data_type, suffix, &globals);
        }
        collect_locals(&f.body, &mut scope_vars);

        if let Some(out) = &mut f.output {
            rename_type(&mut out.data_type, suffix, &globals);
        }
        for stmt in &mut f.body {
            rename_stmt(stmt, suffix, &scope_vars);
        }
    }

    for ca in &mut module.const_asserts {
        rename_expr(&mut ca.condition, suffix, &globals);
    }
}

fn collect_locals(stmts: &[Statement], locals: &mut HashSet<String>) {
    for stmt in stmts {
        match stmt {
            Statement::LetDecl(s) => {
                locals.insert(s.ident.clone());
            }
            Statement::VarDecl(s) => {
                locals.insert(s.ident.clone());
            }
            Statement::ConstDecl(s) => {
                locals.insert(s.ident.clone());
            }
            Statement::Compound(stmts) => collect_locals(stmts, locals),
            Statement::If(s) => {
                collect_locals(&s.body, locals);
                let mut current_else = s.else_.as_deref();
                while let Some(els) = current_else {
                    match els {
                        Else::If(if_s) => {
                            collect_locals(&if_s.body, locals);
                            current_else = if_s.else_.as_deref();
                        }
                        Else::Else(stmts) => {
                            collect_locals(stmts, locals);
                            break;
                        }
                    }
                }
            }
            Statement::Loop(s) => {
                collect_locals(&s.body, locals);
                if let Some(cont) = &s.continuing {
                    collect_locals(&cont.stmts, locals);
                }
            }
            Statement::While(s) => collect_locals(&s.body, locals),
            Statement::Switch(s) => {
                for c in &s.cases {
                    collect_locals(&c.body, locals);
                }
                collect_locals(&s.default, locals);
            }
            Statement::ForLoop(s) => {
                if let Some(init) = &s.header.init {
                    match init {
                        ForLoopInit::VarDecl(d) => {
                            locals.insert(d.ident.clone());
                        }
                        ForLoopInit::LetDecl(d) => {
                            locals.insert(d.ident.clone());
                        }
                        ForLoopInit::ConstDecl(d) => {
                            locals.insert(d.ident.clone());
                        }
                        _ => {}
                    }
                }
                collect_locals(&s.body, locals);
            }
            _ => {}
        }
    }
}

fn rename_type(ty: &mut DataType, suffix: &str, globals: &HashSet<String>) {
    match ty {
        DataType::Array(inner, _size) => {
            let mut new_inner = (**inner).clone();
            rename_type(&mut new_inner, suffix, globals);
            *inner = Rc::new(new_inner);
        }
        DataType::Struct(decl) => {
            if globals.contains(&decl.name) {
                let new_name = format!("{}{}", decl.name, suffix);
                let mut new_members = vec![];
                for m in &decl.members {
                    let mut new_dt = m.data_type.clone();
                    rename_type(&mut new_dt, suffix, globals);
                    new_members.push(StructMember::new(m.attrs.clone(), m.name.clone(), new_dt));
                }
                *decl = StructDecl::new(new_name, new_members);
            }
        }
        DataType::Ptr(view) | DataType::Ref(view) => {
            let mut new_inner = (*view.inner).clone();
            rename_type(&mut new_inner, suffix, globals);
            view.inner = Rc::new(new_inner);
        }
        DataType::FrexpResult(inner) | DataType::ModfResult(inner) => {
            rename_type(inner, suffix, globals);
        }
        _ => {}
    }
}

fn rename_expr(node: &mut ExprNode, suffix: &str, globals: &HashSet<String>) {
    rename_type(&mut node.data_type, suffix, globals);
    match &mut node.expr {
        Expr::Var(v) => {
            if globals.contains(&v.ident) {
                v.ident.push_str(suffix);
            }
        }
        Expr::FnCall(call) => {
            if globals.contains(&call.ident) {
                call.ident.push_str(suffix);
            }
            for arg in &mut call.args {
                rename_expr(arg, suffix, globals);
            }
            for arg in &mut call.template_args {
                rename_type(arg, suffix, globals);
            }
        }
        Expr::TypeCons(tc) => {
            rename_type(&mut tc.data_type, suffix, globals);
            for arg in &mut tc.args {
                rename_expr(arg, suffix, globals);
            }
        }
        Expr::Postfix(pf) => {
            rename_expr(&mut pf.inner, suffix, globals);
            if let Postfix::Index(idx) = &mut pf.postfix {
                rename_expr(idx, suffix, globals);
            }
        }
        Expr::UnOp(u) => {
            rename_expr(&mut u.inner, suffix, globals);
        }
        Expr::BinOp(b) => {
            rename_expr(&mut b.left, suffix, globals);
            rename_expr(&mut b.right, suffix, globals);
        }
        Expr::Lit(_) => {}
    }
}

fn rename_lhs_node(node: &mut LhsExprNode, suffix: &str, globals: &HashSet<String>) {
    rename_type(&mut node.data_type, suffix, globals);
    match &mut node.expr {
        LhsExpr::Ident(name) => {
            if globals.contains(name) {
                name.push_str(suffix);
            }
        }
        LhsExpr::Postfix(inner, pf) => {
            rename_lhs_node(inner, suffix, globals);
            if let Postfix::Index(idx) = pf {
                rename_expr(idx, suffix, globals);
            }
        }
        LhsExpr::Deref(inner) => rename_lhs_node(inner, suffix, globals),
        LhsExpr::AddressOf(inner) => rename_lhs_node(inner, suffix, globals),
    }
}

fn rename_lhs(lhs: &mut AssignmentLhs, suffix: &str, globals: &HashSet<String>) {
    match lhs {
        AssignmentLhs::Expr(node) => rename_lhs_node(node, suffix, globals),
        AssignmentLhs::Phony => {}
    }
}

fn rename_var_decl(s: &mut VarDeclStatement, suffix: &str, globals: &HashSet<String>) {
    if globals.contains(&s.ident) {
        s.ident.push_str(suffix);
    }
    if let Some(ty) = &mut s.data_type {
        rename_type(ty, suffix, globals);
    }
    if let Some(init) = &mut s.initializer {
        rename_expr(init, suffix, globals);
    }
}

fn rename_let_decl(s: &mut LetDeclStatement, suffix: &str, globals: &HashSet<String>) {
    if globals.contains(&s.ident) {
        s.ident.push_str(suffix);
    }
    if let Some(ty) = &mut s.data_type {
        rename_type(ty, suffix, globals);
    }
    rename_expr(&mut s.initializer, suffix, globals);
}

fn rename_const_decl(s: &mut ConstDeclStatement, suffix: &str, globals: &HashSet<String>) {
    if globals.contains(&s.ident) {
        s.ident.push_str(suffix);
    }
    if let Some(ty) = &mut s.data_type {
        rename_type(ty, suffix, globals);
    }
    rename_expr(&mut s.initializer, suffix, globals);
}

fn rename_assign(s: &mut AssignmentStatement, suffix: &str, globals: &HashSet<String>) {
    rename_lhs(&mut s.lhs, suffix, globals);
    rename_expr(&mut s.rhs, suffix, globals);
}

fn rename_call_stmt(s: &mut FnCallStatement, suffix: &str, globals: &HashSet<String>) {
    if globals.contains(&s.ident) {
        s.ident.push_str(suffix);
    }
    for arg in &mut s.args {
        rename_expr(arg, suffix, globals);
    }
}

fn rename_if_statement(s: &mut IfStatement, suffix: &str, globals: &HashSet<String>) {
    rename_expr(&mut s.condition, suffix, globals);
    for bs in &mut s.body {
        rename_stmt(bs, suffix, globals);
    }
    if let Some(els) = &mut s.else_ {
        match &mut **els {
            Else::If(if_s) => rename_if_statement(if_s, suffix, globals),
            Else::Else(stmts) => {
                for bs in stmts {
                    rename_stmt(bs, suffix, globals);
                }
            }
        }
    }
}

fn rename_stmt(stmt: &mut Statement, suffix: &str, globals: &HashSet<String>) {
    match stmt {
        Statement::LetDecl(s) => rename_let_decl(s, suffix, globals),
        Statement::VarDecl(s) => rename_var_decl(s, suffix, globals),
        Statement::ConstDecl(s) => rename_const_decl(s, suffix, globals),
        Statement::Assignment(s) => rename_assign(s, suffix, globals),
        Statement::Increment(s) => rename_lhs(&mut s.lhs, suffix, globals),
        Statement::Decrement(s) => rename_lhs(&mut s.lhs, suffix, globals),
        Statement::Compound(stmts) => {
            for s in stmts {
                rename_stmt(s, suffix, globals);
            }
        }
        Statement::If(s) => rename_if_statement(s, suffix, globals),
        Statement::Return(s) => {
            if let Some(val) = &mut s.value {
                rename_expr(val, suffix, globals);
            }
        }
        Statement::Loop(s) => {
            for bs in &mut s.body {
                rename_stmt(bs, suffix, globals);
            }
            if let Some(cont) = &mut s.continuing {
                for bs in &mut cont.stmts {
                    rename_stmt(bs, suffix, globals);
                }
                if let Some(br) = &mut cont.break_if {
                    rename_expr(br, suffix, globals);
                }
            }
        }
        Statement::While(s) => {
            rename_expr(&mut s.condition, suffix, globals);
            for bs in &mut s.body {
                rename_stmt(bs, suffix, globals);
            }
        }
        Statement::Switch(s) => {
            rename_expr(&mut s.selector, suffix, globals);
            for c in &mut s.cases {
                rename_expr(&mut c.selector, suffix, globals);
                for bs in &mut c.body {
                    rename_stmt(bs, suffix, globals);
                }
            }
            for bs in &mut s.default {
                rename_stmt(bs, suffix, globals);
            }
        }
        Statement::ForLoop(s) => {
            if let Some(init) = &mut s.header.init {
                match init {
                    ForLoopInit::VarDecl(d) => rename_var_decl(d, suffix, globals),
                    ForLoopInit::LetDecl(d) => rename_let_decl(d, suffix, globals),
                    ForLoopInit::ConstDecl(d) => rename_const_decl(d, suffix, globals),
                    ForLoopInit::Assignment(a) => rename_assign(a, suffix, globals),
                    ForLoopInit::Increment(i) => rename_lhs(&mut i.lhs, suffix, globals),
                    ForLoopInit::Decrement(d) => rename_lhs(&mut d.lhs, suffix, globals),
                    ForLoopInit::Call(c) => rename_call_stmt(c, suffix, globals),
                }
            }
            if let Some(cond) = &mut s.header.condition {
                rename_expr(cond, suffix, globals);
            }
            if let Some(upd) = &mut s.header.update {
                match upd {
                    ForLoopUpdate::Assignment(a) => rename_assign(a, suffix, globals),
                    ForLoopUpdate::Increment(i) => rename_lhs(&mut i.lhs, suffix, globals),
                    ForLoopUpdate::Decrement(d) => rename_lhs(&mut d.lhs, suffix, globals),
                    ForLoopUpdate::Call(c) => rename_call_stmt(c, suffix, globals),
                }
            }
            for bs in &mut s.body {
                rename_stmt(bs, suffix, globals);
            }
        }
        Statement::FnCall(s) => rename_call_stmt(s, suffix, globals),
        Statement::ConstAssert(s) => rename_expr(&mut s.condition, suffix, globals),
        _ => {}
    }
}
