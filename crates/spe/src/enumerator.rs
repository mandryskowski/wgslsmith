use ast::types::DataType;
use ast::*;
#[derive(Clone, Debug, PartialEq, Eq)]
enum HoleType {
    Declaration { mutable: bool },
    Usage { is_lvalue: bool },
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
}
impl Context {
    fn new() -> Self {
        Context {
            holes: vec![],
            assignments: None,
            hole_counter: 0,
            scope_stack: vec![0],
            scope_parents: vec![0],
            next_scope_id: 1,
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
    fn process_decl(&mut self, name: &mut String, data_type: &DataType, mutable: bool) {
        if self.assignments.is_none() {
            self.holes.push(Hole {
                hole_type: HoleType::Declaration { mutable },
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
                hole_type: HoleType::Usage { is_lvalue },
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
        ctx.process_decl(&mut decl.ident, &ty, false);
    }
    for decl in &mut module.vars {
        let ty = decl.data_type.clone();
        let mut is_mutable = true;
        if let Some(qualifier) = &decl.qualifier {
            let access_mode = qualifier
                .access_mode
                .unwrap_or_else(|| qualifier.storage_class.default_access_mode());
            if access_mode == ast::AccessMode::Read {
                is_mutable = false;
            }
        }
        ctx.process_decl(&mut decl.name, &ty, is_mutable);
    }
    for func in &mut module.functions {
        visit_fn(func, ctx);
    }
}
fn visit_fn(func: &mut FnDecl, ctx: &mut Context) {
    ctx.enter_scope();
    for input in &mut func.inputs {
        let ty = input.data_type.clone();
        ctx.process_decl(&mut input.name, &ty, false);
    }
    for stmt in &mut func.body {
        visit_stmt(stmt, ctx);
    }
    ctx.exit_scope();
}
fn visit_stmt(stmt: &mut Statement, ctx: &mut Context) {
    match stmt {
        Statement::LetDecl(s) => {
            visit_expr_node(&mut s.initializer, ctx);
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, false);
        }
        Statement::VarDecl(s) => {
            if let Some(init) = &mut s.initializer {
                visit_expr_node(init, ctx);
            }
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, true);
        }
        Statement::ConstDecl(s) => {
            visit_expr_node(&mut s.initializer, ctx);
            let ty = s.inferred_type().clone();
            ctx.process_decl(&mut s.ident, &ty, false);
        }
        Statement::Assignment(s) => {
            visit_lhs(&mut s.lhs, ctx);
            visit_expr_node(&mut s.rhs, ctx);
        }
        Statement::Compound(stmts) => {
            ctx.enter_scope();
            for s in stmts {
                visit_stmt(s, ctx);
            }
            ctx.exit_scope();
        }
        Statement::If(s) => {
            visit_expr_node(&mut s.condition, ctx);
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
                visit_expr_node(val, ctx);
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
                    visit_expr_node(br, ctx);
                }
                ctx.exit_scope();
            }
            ctx.exit_scope();
        }
        Statement::While(s) => {
            visit_expr_node(&mut s.condition, ctx);
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
                            visit_expr_node(i, ctx);
                        }
                        let ty = d.inferred_type().clone();
                        ctx.process_decl(&mut d.ident, &ty, true);
                    }
                }
            }
            if let Some(cond) = &mut s.header.condition {
                visit_expr_node(cond, ctx);
            }
            if let Some(upd) = &mut s.header.update {
                match upd {
                    ForLoopUpdate::Assignment(a) => {
                        visit_lhs(&mut a.lhs, ctx);
                        visit_expr_node(&mut a.rhs, ctx);
                    }
                    ForLoopUpdate::Increment(i) => visit_lhs(&mut i.lhs, ctx),
                    ForLoopUpdate::Decrement(d) => visit_lhs(&mut d.lhs, ctx),
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
                visit_expr_node(arg, ctx);
            }
        }
        Statement::Increment(s) => visit_lhs(&mut s.lhs, ctx),
        Statement::Decrement(s) => visit_lhs(&mut s.lhs, ctx),
        Statement::Switch(s) => {
            visit_expr_node(&mut s.selector, ctx);
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
    visit_expr_node(&mut stmt.condition, ctx);
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
                visit_expr_node(idx_expr, ctx);
            }
        }
        LhsExpr::Deref(inner) => visit_lhs_expr_node(inner, ctx),
        LhsExpr::AddressOf(inner) => visit_lhs_expr_node(inner, ctx),
    }
}
fn visit_expr_node(node: &mut ExprNode, ctx: &mut Context) {
    match &mut node.expr {
        Expr::Var(v) => {
            let ty = node.data_type.dereference().clone();
            ctx.process_usage(&mut v.ident, &ty, false);
        }
        Expr::FnCall(call) => {
            for arg in &mut call.args {
                visit_expr_node(arg, ctx);
            }
        }
        Expr::BinOp(op) => {
            visit_expr_node(&mut op.left, ctx);
            visit_expr_node(&mut op.right, ctx);
        }
        Expr::UnOp(op) => {
            visit_expr_node(&mut op.inner, ctx);
        }
        Expr::Postfix(p) => {
            visit_expr_node(&mut p.inner, ctx);
            if let Postfix::Index(idx) = &mut p.postfix {
                visit_expr_node(idx, ctx);
            }
        }
        Expr::TypeCons(t) => {
            for arg in &mut t.args {
                visit_expr_node(arg, ctx);
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
        for id in 0..=next_available {
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
                let prev_is_mut = match prev_hole.hole_type {
                    HoleType::Declaration { mutable } => mutable,
                    HoleType::Usage { .. } => true,
                };
                if let HoleType::Usage { is_lvalue: true } = hole.hole_type {
                    if !prev_is_mut {
                        return false;
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
    let mut ctx = Context::new();
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
            HoleType::Usage { is_lvalue } => {
                let mut choices = 0;
                for prev_hole in &ctx.holes[..i] {
                    if let HoleType::Declaration { mutable } = prev_hole.hole_type {
                        if prev_hole.data_type.dereference() == hole.data_type.dereference() {
                            if *is_lvalue && !mutable {
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
    let mut ctx = Context::new();
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
    let original_idx = enumerator
        .results
        .iter()
        .position(|r| r == &original_assignment);

    (ctx.holes.len(), enumerator.results, original_idx)
}

pub fn filter_module(module: &mut Module) -> bool {
    let mut compute_found = false;
    let mut live_functions = std::collections::HashSet::new();

    for func in &module.functions {
        let is_compute = func
            .attrs
            .iter()
            .any(|a| matches!(a, FnAttr::Stage(ShaderStage::Compute)));
        if is_compute {
            compute_found = true;
            collect_live_functions(&func.name, module, &mut live_functions);
        }
    }

    if !compute_found {
        return false;
    }

    module
        .functions
        .retain(|f| live_functions.contains(&f.name));
    true
}

fn collect_live_functions(
    name: &str,
    module: &Module,
    live_functions: &mut std::collections::HashSet<String>,
) {
    if !live_functions.insert(name.to_string()) {
        return;
    }

    let func = match module.functions.iter().find(|f| f.name == name) {
        Some(f) => f,
        None => return,
    };

    struct CallVisitor<'a> {
        module: &'a Module,
        live_functions: &'a mut std::collections::HashSet<String>,
    }

    impl<'a> CallVisitor<'a> {
        fn visit_expr(&mut self, expr: &ExprNode) {
            match &expr.expr {
                Expr::FnCall(call) => {
                    collect_live_functions(&call.ident, self.module, self.live_functions);
                    for arg in &call.args {
                        self.visit_expr(arg);
                    }
                }
                Expr::BinOp(op) => {
                    self.visit_expr(&op.left);
                    self.visit_expr(&op.right);
                }
                Expr::UnOp(op) => {
                    self.visit_expr(&op.inner);
                }
                Expr::Postfix(p) => {
                    self.visit_expr(&p.inner);
                    if let Postfix::Index(idx) = &p.postfix {
                        self.visit_expr(idx);
                    }
                }
                Expr::TypeCons(t) => {
                    for arg in &t.args {
                        self.visit_expr(arg);
                    }
                }
                _ => {}
            }
        }

        fn visit_stmt(&mut self, stmt: &Statement) {
            match stmt {
                Statement::LetDecl(s) => self.visit_expr(&s.initializer),
                Statement::VarDecl(s) => {
                    if let Some(init) = &s.initializer {
                        self.visit_expr(init);
                    }
                }
                Statement::ConstDecl(s) => self.visit_expr(&s.initializer),
                Statement::Assignment(s) => {
                    self.visit_lhs(&s.lhs);
                    self.visit_expr(&s.rhs);
                }
                Statement::Compound(stmts) => {
                    for s in stmts {
                        self.visit_stmt(s);
                    }
                }
                Statement::If(s) => {
                    self.visit_expr(&s.condition);
                    for bs in &s.body {
                        self.visit_stmt(bs);
                    }
                    if let Some(else_block) = &s.else_ {
                        match else_block.as_ref() {
                            Else::If(if_stmt) => self.visit_if(if_stmt),
                            Else::Else(stmts) => {
                                for es in stmts {
                                    self.visit_stmt(es);
                                }
                            }
                        }
                    }
                }
                Statement::Return(s) => {
                    if let Some(val) = &s.value {
                        self.visit_expr(val);
                    }
                }
                Statement::Loop(s) => {
                    for bs in &s.body {
                        self.visit_stmt(bs);
                    }
                    if let Some(cont) = &s.continuing {
                        for cs in &cont.stmts {
                            self.visit_stmt(cs);
                        }
                        if let Some(br) = &cont.break_if {
                            self.visit_expr(br);
                        }
                    }
                }
                Statement::While(s) => {
                    self.visit_expr(&s.condition);
                    for bs in &s.body {
                        self.visit_stmt(bs);
                    }
                }
                Statement::ForLoop(s) => {
                    if let Some(init) = &s.header.init {
                        match init {
                            ForLoopInit::VarDecl(d) => {
                                if let Some(i) = &d.initializer {
                                    self.visit_expr(i);
                                }
                            }
                        }
                    }
                    if let Some(cond) = &s.header.condition {
                        self.visit_expr(cond);
                    }
                    if let Some(upd) = &s.header.update {
                        match upd {
                            ForLoopUpdate::Assignment(a) => {
                                self.visit_lhs(&a.lhs);
                                self.visit_expr(&a.rhs);
                            }
                            ForLoopUpdate::Increment(i) => self.visit_lhs(&i.lhs),
                            ForLoopUpdate::Decrement(d) => self.visit_lhs(&d.lhs),
                        }
                    }
                    for bs in &s.body {
                        self.visit_stmt(bs);
                    }
                }
                Statement::FnCall(s) => {
                    collect_live_functions(&s.ident, self.module, self.live_functions);
                    for arg in &s.args {
                        self.visit_expr(arg);
                    }
                }
                Statement::Increment(s) => self.visit_lhs(&s.lhs),
                Statement::Decrement(s) => self.visit_lhs(&s.lhs),
                Statement::Switch(s) => {
                    self.visit_expr(&s.selector);
                    for case in &s.cases {
                        for bs in &case.body {
                            self.visit_stmt(bs);
                        }
                    }
                    for bs in &s.default {
                        self.visit_stmt(bs);
                    }
                }
                _ => {}
            }
        }

        fn visit_if(&mut self, stmt: &IfStatement) {
            self.visit_expr(&stmt.condition);
            for s in &stmt.body {
                self.visit_stmt(s);
            }
            if let Some(else_block) = &stmt.else_ {
                match else_block.as_ref() {
                    Else::If(s) => self.visit_if(s),
                    Else::Else(stmts) => {
                        for s in stmts {
                            self.visit_stmt(s);
                        }
                    }
                }
            }
        }

        fn visit_lhs(&mut self, lhs: &AssignmentLhs) {
            if let AssignmentLhs::Expr(node) = lhs {
                self.visit_lhs_expr(node);
            }
        }

        fn visit_lhs_expr(&mut self, node: &LhsExprNode) {
            match &node.expr {
                LhsExpr::Postfix(inner, postfix) => {
                    self.visit_lhs_expr(inner);
                    if let Postfix::Index(idx_expr) = postfix {
                        self.visit_expr(idx_expr);
                    }
                }
                LhsExpr::Deref(inner) => self.visit_lhs_expr(inner),
                LhsExpr::AddressOf(inner) => self.visit_lhs_expr(inner),
                _ => {}
            }
        }
    }

    let mut visitor = CallVisitor {
        module,
        live_functions,
    };

    for stmt in &func.body {
        visitor.visit_stmt(stmt);
    }
}

pub fn apply_assignment(module: &Module, assigns: &[usize]) -> String {
    let mut case_module = module.clone();
    let mut apply_ctx = Context::new();
    apply_ctx.assignments = Some(assigns.to_vec());
    visit_module(&mut case_module, &mut apply_ctx);
    let mut out_str = String::new();
    ast::writer::Writer::default()
        .write_module(&mut out_str, &case_module)
        .unwrap();
    out_str
}
