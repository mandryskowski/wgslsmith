use crate::vertex_reachable::get_vertex_reachable_functions;
use ast::types::DataType;
use ast::*;
use rand::seq::SliceRandom;

#[derive(Clone, Debug, PartialEq, Eq)]
enum HoleType {
    Declaration {
        mutable: bool,
        is_const: bool,
        banned_from_vertex: bool,
    },
    Usage {
        is_lvalue: bool,
        requires_const: bool,
        in_vertex_stage: bool,
    },
}
#[derive(Clone, Debug)]
struct Hole {
    hole_type: HoleType,
    data_type: DataType,
    scope_id: usize,
    #[allow(dead_code)]
    original_name: String,
}
struct Context {
    holes: Vec<Hole>,
    assignments: Option<Vec<usize>>,
    hole_counter: usize,
    scope_stack: Vec<usize>,
    scope_parents: Vec<usize>,
    next_scope_id: usize,
    in_const_context: bool,
    in_vertex_stage: bool,
    vertex_reachable_functions: std::collections::HashSet<String>,
}
impl Context {
    fn new(module: Option<&Module>) -> Self {
        let vertex_reachable_functions = module
            .map(get_vertex_reachable_functions)
            .unwrap_or_default();

        Context {
            holes: vec![],
            assignments: None,
            hole_counter: 0,
            scope_stack: vec![0],
            scope_parents: vec![0],
            next_scope_id: 1,
            in_const_context: false,
            in_vertex_stage: false,
            vertex_reachable_functions,
        }
    }
    fn enter_scope(&mut self) {
        let current = *self.scope_stack.last().unwrap();
        if self.next_scope_id >= self.scope_parents.len() {
            self.scope_parents.resize(self.next_scope_id + 1, 0);
        }
        self.scope_parents[self.next_scope_id] = current;
        self.scope_stack.push(self.next_scope_id);
        self.next_scope_id += 1;
    }
    fn exit_scope(&mut self) {
        self.scope_stack.pop();
    }
    fn current_scope(&self) -> usize {
        *self.scope_stack.last().unwrap()
    }
    fn process_decl(
        &mut self,
        name: &mut String,
        data_type: &DataType,
        mutable: bool,
        is_const: bool,
        banned_from_vertex: bool,
    ) {
        if self.assignments.is_none() {
            self.holes.push(Hole {
                hole_type: HoleType::Declaration {
                    mutable,
                    is_const,
                    banned_from_vertex,
                },
                data_type: data_type.clone(),
                scope_id: self.current_scope(),
                original_name: name.clone(),
            });
        } else if let Some(assigns) = &self.assignments {
            if self.hole_counter < assigns.len() {
                *name = format!("v{}", assigns[self.hole_counter]);
            }
            self.hole_counter += 1;
        }
    }
    fn process_usage(&mut self, name: &mut String, data_type: &DataType, is_lvalue: bool) {
        if self.assignments.is_none() {
            self.holes.push(Hole {
                hole_type: HoleType::Usage {
                    is_lvalue,
                    requires_const: self.in_const_context,
                    in_vertex_stage: self.in_vertex_stage,
                },
                data_type: data_type.clone(),
                scope_id: self.current_scope(),
                original_name: name.clone(),
            });
        } else if let Some(assigns) = &self.assignments {
            if self.hole_counter < assigns.len() {
                *name = format!("v{}", assigns[self.hole_counter]);
            }
            self.hole_counter += 1;
        }
    }
}

fn visit_module(module: &mut Module, ctx: &mut Context) {
    for decl in &mut module.consts {
        let ty = decl.inferred_type().clone();
        ctx.process_decl(&mut decl.ident, &ty, false, true, false);
    }
    for decl in &mut module.vars {
        let ty = decl.data_type.clone();
        let mut is_mutable = true;
        let mut banned_from_vertex = false;
        if let Some(qualifier) = &decl.qualifier {
            let access_mode = qualifier
                .access_mode
                .unwrap_or_else(|| qualifier.storage_class.default_access_mode());
            if access_mode == ast::AccessMode::Read {
                is_mutable = false;
            }
            if qualifier.storage_class == ast::StorageClass::Storage
                && access_mode != ast::AccessMode::Read
            {
                banned_from_vertex = true;
            }
        }

        if let ast::DataType::Texture(ast::TextureType::Storage { access, .. }) = ty.dereference() {
            if *access != ast::AccessMode::Read {
                banned_from_vertex = true;
            }
        }

        if let Some(view) = ty.as_memory_view() {
            if view.access_mode == ast::AccessMode::Read {
                is_mutable = false;
            }
            if view.storage_class == ast::StorageClass::Storage
                && view.access_mode != ast::AccessMode::Read
            {
                banned_from_vertex = true;
            }
        }
        ctx.process_decl(&mut decl.name, &ty, is_mutable, false, banned_from_vertex);
    }
    for func in &mut module.functions {
        visit_fn(func, ctx);
    }
}
fn visit_fn(func: &mut FnDecl, ctx: &mut Context) {
    let prev_vertex = ctx.in_vertex_stage;
    if ctx.vertex_reachable_functions.contains(&func.name) {
        ctx.in_vertex_stage = true;
    }
    ctx.enter_scope();
    for input in &mut func.inputs {
        let ty = input.data_type.clone();
        ctx.process_decl(&mut input.name, &ty, false, false, false);
    }
    for stmt in &mut func.body {
        visit_stmt(stmt, ctx);
    }
    ctx.exit_scope();
    ctx.in_vertex_stage = prev_vertex;
}
fn visit_stmt(stmt: &mut Statement, ctx: &mut Context) {
    match stmt {
        Statement::LetDecl(s) => {
            visit_expr_node(&mut s.initializer, ctx, false);
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, false, false, false);
        }
        Statement::VarDecl(s) => {
            if let Some(init) = &mut s.initializer {
                visit_expr_node(init, ctx, false);
            }
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, true, false, false);
        }
        Statement::ConstDecl(s) => {
            let prev = ctx.in_const_context;
            ctx.in_const_context = true;
            visit_expr_node(&mut s.initializer, ctx, false);
            ctx.in_const_context = prev;
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, false, true, false);
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
                        ctx.process_decl(&mut d.ident, &ty, true, false, false);
                    }
                    ForLoopInit::LetDecl(d) => {
                        visit_expr_node(&mut d.initializer, ctx, false);
                        let ty = d.inferred_type().clone();
                        ctx.process_decl(&mut d.ident, &ty, false, false, false);
                    }
                    ForLoopInit::ConstDecl(d) => {
                        let prev = ctx.in_const_context;
                        ctx.in_const_context = true;
                        visit_expr_node(&mut d.initializer, ctx, false);
                        ctx.in_const_context = prev;
                        let ty = d.inferred_type().clone();
                        ctx.process_decl(&mut d.ident, &ty, false, true, false);
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
            ctx.process_usage(name, &ty, true);
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
fn visit_expr_node(node: &mut ExprNode, ctx: &mut Context, needs_ref: bool) {
    match &mut node.expr {
        Expr::Var(v) => {
            let ty = node.data_type.dereference().clone();
            ctx.process_usage(&mut v.ident, &ty, needs_ref);
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
            let is_addr_of = matches!(op.op, UnOp::AddressOf);
            visit_expr_node(&mut op.inner, ctx, needs_ref || is_addr_of);
        }
        Expr::Postfix(p) => {
            visit_expr_node(&mut p.inner, ctx, needs_ref);
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
struct Enumerator {
    holes: Vec<Hole>,
    results: Vec<Vec<usize>>,
    scope_parents: Vec<usize>,
    limit: Option<usize>,
    step_count: usize,
}
impl Enumerator {
    fn enumerate(&mut self, current: &mut Vec<usize>) {
        self.step_count += 1;
        if self.step_count > 100_000 {
            return;
        }
        if let Some(lim) = self.limit {
            if self.results.len() >= lim {
                return;
            }
        }
        if current.len() == self.holes.len() {
            self.results.push(current.clone());
            return;
        }
        let hole_idx = current.len();

        let max_id = current
            .iter()
            .max()
            .copied()
            .map(|m| m as i32)
            .unwrap_or(-1);
        let next_available = (max_id + 1) as usize;

        let mut ids: Vec<usize> = (0..=next_available).collect();
        ids.shuffle(&mut rand::thread_rng());

        for id in ids {
            if self.is_valid_assignment(hole_idx, id, current) {
                current.push(id);
                self.enumerate(current);
                current.pop();
            }
        }
    }
    fn is_ancestor(&self, possible_ancestor: usize, mut node: usize) -> bool {
        if possible_ancestor == node {
            return true;
        }

        while node != 0 {
            let parent = self.scope_parents[node];
            if parent == possible_ancestor {
                return true;
            }
            if parent == node {
                break;
            }
            node = parent;
        }
        possible_ancestor == 0
    }
    fn is_valid_assignment(&self, hole_idx: usize, id: usize, current: &[usize]) -> bool {
        let hole = &self.holes[hole_idx];
        for (prev_idx, &prev_id) in current.iter().enumerate() {
            if prev_id == id {
                let prev_hole = &self.holes[prev_idx];
                if prev_hole.data_type.dereference() != hole.data_type.dereference() {
                    return false;
                }
                if let HoleType::Declaration {
                    mutable,
                    is_const,
                    banned_from_vertex,
                } = prev_hole.hole_type
                {
                    if let HoleType::Usage {
                        is_lvalue,
                        requires_const,
                        in_vertex_stage,
                    } = hole.hole_type
                    {
                        if is_lvalue && !mutable {
                            return false;
                        }
                        if requires_const && !is_const {
                            return false;
                        }
                        if in_vertex_stage && banned_from_vertex {
                            return false;
                        }
                    }
                }
            }
        }
        match &hole.hole_type {
            HoleType::Declaration { .. } => {
                let mut is_reused = false;
                let mut visible_and_reusable = false;
                for (prev_idx, &prev_id) in current.iter().enumerate() {
                    if prev_id == id {
                        is_reused = true;
                        if let HoleType::Declaration { .. } = self.holes[prev_idx].hole_type {
                            let prev_scope = self.holes[prev_idx].scope_id;
                            if self.is_ancestor(prev_scope, hole.scope_id) {
                                if prev_scope == hole.scope_id {
                                    return false;
                                }
                                visible_and_reusable = true;
                            }
                        }
                    }
                }
                if is_reused && !visible_and_reusable {
                    return false;
                }
            }
            HoleType::Usage { .. } => {
                let mut valid_decl_found = false;
                for (prev_idx, &prev_id) in current.iter().enumerate() {
                    if prev_id == id {
                        if let HoleType::Declaration { .. } = self.holes[prev_idx].hole_type {
                            let decl_scope = self.holes[prev_idx].scope_id;
                            let usage_scope = hole.scope_id;
                            if self.is_ancestor(decl_scope, usage_scope) {
                                valid_decl_found = true;
                                break;
                            }
                        }
                    }
                }
                if !valid_decl_found {
                    return false;
                }
            }
        }
        true
    }
}
pub fn estimate_enumerations(module: &Module) -> usize {
    let mut ctx = Context::new(Some(module));
    let mut analyze_module = module.clone();
    visit_module(&mut analyze_module, &mut ctx);

    let mut bound: usize = 1;
    for (i, hole) in ctx.holes.iter().enumerate() {
        match &hole.hole_type {
            HoleType::Declaration { .. } => {
                let mut choices = 1;
                for prev_hole in &ctx.holes[..i] {
                    if let HoleType::Declaration { .. } = prev_hole.hole_type {
                        if prev_hole.data_type.dereference() == hole.data_type.dereference() {
                            choices += 1;
                        }
                    }
                }
                bound = bound.saturating_mul(choices);
            }
            HoleType::Usage {
                is_lvalue,
                requires_const,
                in_vertex_stage,
            } => {
                let mut choices = 0;
                for prev_hole in &ctx.holes[..i] {
                    if let HoleType::Declaration {
                        mutable,
                        is_const,
                        banned_from_vertex,
                    } = prev_hole.hole_type
                    {
                        if prev_hole.data_type.dereference() == hole.data_type.dereference() {
                            if *is_lvalue && !mutable {
                                continue;
                            }
                            if *requires_const && !is_const {
                                continue;
                            }
                            if *in_vertex_stage && banned_from_vertex {
                                continue;
                            }
                            choices += 1;
                        }
                    }
                }
                if choices == 0 {
                    return 0; // unsolvable constraint
                }
                bound = bound.saturating_mul(choices);
            }
        }
    }
    bound
}

fn get_original_assignment(holes: &[Hole], scope_parents: &[usize]) -> Vec<usize> {
    let mut name_to_id: std::collections::HashMap<(usize, String), usize> =
        std::collections::HashMap::new();
    let mut current_assignment = Vec::new();
    let mut next_available_id = 0;

    for hole in holes {
        match &hole.hole_type {
            HoleType::Declaration { .. } => {
                let id = next_available_id;
                next_available_id += 1;
                name_to_id.insert((hole.scope_id, hole.original_name.clone()), id);
                current_assignment.push(id);
            }
            HoleType::Usage { .. } => {
                let mut cur_scope = hole.scope_id;
                let mut found_id = None;
                loop {
                    if let Some(&id) = name_to_id.get(&(cur_scope, hole.original_name.clone())) {
                        found_id = Some(id);
                        break;
                    }
                    if cur_scope == 0 {
                        break;
                    }
                    cur_scope = scope_parents[cur_scope];
                }
                if let Some(id) = found_id {
                    current_assignment.push(id);
                } else {
                    panic!(
                        "No declaration found for hole {} in scope {}",
                        hole.original_name, hole.scope_id
                    );
                }
            }
        }
    }
    current_assignment
}

pub fn get_enumerations(
    module: &Module,
    limit: Option<usize>,
) -> (usize, Vec<Vec<usize>>, Option<usize>) {
    let mut ctx = Context::new(Some(module));
    let mut analyze_module = module.clone();
    visit_module(&mut analyze_module, &mut ctx);

    let mut enumerator = Enumerator {
        holes: ctx.holes.clone(),
        results: vec![],
        scope_parents: ctx.scope_parents.clone(),
        limit,
        step_count: 0,
    };
    enumerator.enumerate(&mut vec![]);

    let original_assignment = get_original_assignment(&ctx.holes, &ctx.scope_parents);

    let mut original_idx = enumerator
        .results
        .iter()
        .position(|r| r == &original_assignment);

    if original_idx.is_none() {
        enumerator.results.insert(0, original_assignment.clone());
        original_idx = Some(0);
    }

    (ctx.holes.len(), enumerator.results, original_idx)
}

pub fn apply_assignment(module: &Module, assigns: &[usize]) -> String {
    let mut case_module = module.clone();
    let mut apply_ctx = Context::new(None);
    apply_ctx.assignments = Some(assigns.to_vec());
    visit_module(&mut case_module, &mut apply_ctx);
    let mut out_str = String::new();
    ast::writer::Writer::default()
        .write_module(&mut out_str, &case_module)
        .unwrap();
    out_str
}
