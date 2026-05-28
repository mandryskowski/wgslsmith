mod safe_wrappers;

pub mod analysis;
pub mod cli;

use std::collections::HashSet;
use std::fmt::Display;

use ast::types::{DataType, MemoryViewType, ScalarType};
use ast::*;

pub struct ReconditionResult {
    pub ast: Module,
    pub loop_count: u32,
}

#[derive(Hash, PartialEq, Eq)]
enum Wrapper {
    ExtractBits(DataType),
    InsertBits(DataType),
    FloatOp(DataType),
    FloatDivide(DataType),
    Smoothstep(DataType),
    Normalize(DataType),
    Select(DataType, DataType),
    Mod(DataType),
    Index(DataType),
    Pack2x16float,
    Acos(DataType),
    Acosh(DataType),
    Asin(DataType),
    Atanh(DataType),
}

impl Wrapper {
    fn gen_fn_decl(&self, unstable_float: bool) -> FnDecl {
        let name = self.to_string();
        match self {
            Wrapper::ExtractBits(ty) => {
                if ty.is_signed_int() {
                    safe_wrappers::extract_bits(name, ty)
                } else {
                    safe_wrappers::extract_bits_unsigned(name, ty)
                }
            }
            Wrapper::InsertBits(ty) => safe_wrappers::insert_bits(name, ty),
            Wrapper::FloatOp(ty) => {
                if unstable_float {
                    safe_wrappers::float_noop(name, ty)
                } else {
                    safe_wrappers::float(name, ty)
                }
            }
            Wrapper::FloatDivide(ty) => safe_wrappers::float_divide(name, ty),
            Wrapper::Smoothstep(ty) => safe_wrappers::smoothstep(name, ty),
            Wrapper::Normalize(ty) => safe_wrappers::normalize(name, ty),
            Wrapper::Select(ty, cond_ty) => safe_wrappers::select(name, ty, cond_ty),
            Wrapper::Mod(ty) => safe_wrappers::modulo(name, ty),
            Wrapper::Index(ty) => safe_wrappers::index(name, ty),
            Wrapper::Pack2x16float => safe_wrappers::pack2x16float(name),
            Wrapper::Acos(ty) => {
                safe_wrappers::math_bounds(name, "acos", ty, Some(-1.0), Some(1.0))
            }
            Wrapper::Acosh(ty) => safe_wrappers::math_bounds(name, "acosh", ty, Some(1.0), None),
            Wrapper::Asin(ty) => {
                safe_wrappers::math_bounds(name, "asin", ty, Some(-1.0), Some(1.0))
            }
            Wrapper::Atanh(ty) => {
                safe_wrappers::math_bounds(name, "atanh", ty, Some(-1.0), Some(1.0))
            }
        }
    }
}

impl Display for Wrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // helper func
        let write_type = |f: &mut std::fmt::Formatter<'_>, ty: &DataType| match ty {
            DataType::Scalar(s) => write!(f, "{s}"),
            DataType::Vector(n, s) => write!(f, "vec{n}_{s}"),
            _ => unimplemented!("no wrappers available for expressions of type `{ty}`"),
        };

        write!(f, "_wgslsmith_")?;

        match self {
            Wrapper::Select(ty, cond_ty) => {
                write!(f, "select_")?;
                write_type(f, ty)?;
                write!(f, "_")?;
                write_type(f, cond_ty)
            }
            Wrapper::Pack2x16float => write!(f, "pack2x16float"),
            other => {
                let (name, ty) = match other {
                    Wrapper::ExtractBits(ty) => ("extract_bits", ty),
                    Wrapper::InsertBits(ty) => ("insert_bits", ty),
                    Wrapper::FloatOp(ty) => ("f_op", ty),
                    Wrapper::FloatDivide(ty) => ("div", ty),
                    Wrapper::Smoothstep(ty) => ("smoothstep", ty),
                    Wrapper::Normalize(ty) => ("normalize", ty),
                    Wrapper::Mod(ty) => ("mod", ty),
                    Wrapper::Index(ty) => ("index", ty),
                    Wrapper::Acos(ty) => ("acos", ty),
                    Wrapper::Acosh(ty) => ("acosh", ty),
                    Wrapper::Asin(ty) => ("asin", ty),
                    Wrapper::Atanh(ty) => ("atanh", ty),
                    Wrapper::Select(..) | Wrapper::Pack2x16float => unreachable!(),
                };

                write!(f, "{name}_")?;
                write_type(f, ty)
            }
        }
    }
}

#[derive(Default)]
pub struct Options {
    pub only_loops: bool,
    pub unstable_float: bool,
}

pub fn recondition(ast: Module) -> Module {
    recondition_with(ast, Options::default())
}

pub fn recondition_with(mut ast: Module, options: Options) -> Module {
    let mut reconditioner = Reconditioner::new(options);

    // Abstract numerics
    ast = concretizer::concretize(ast);

    let functions = ast
        .functions
        .into_iter()
        .map(|f| reconditioner.recondition_fn(f))
        .collect::<Vec<_>>();

    ast.functions = reconditioner
        .wrappers
        .iter()
        .map(|w| w.gen_fn_decl(reconditioner.unstable_float))
        .chain(functions)
        .collect();

    if reconditioner.loop_var > 0 {
        ast.vars.push(GlobalVarDecl {
            attrs: vec![],
            data_type: DataType::array(ScalarType::U32, Some(reconditioner.loop_var)),
            name: "LOOP_COUNTERS".into(),
            initializer: None,
            qualifier: Some(VarQualifier {
                storage_class: StorageClass::Private,
                access_mode: None,
            }),
        });
    }

    ast
}

struct Reconditioner {
    loop_var: u32,
    wrappers: HashSet<Wrapper>,
    only_loops: bool,
    unstable_float: bool,
}

impl Reconditioner {
    fn new(options: Options) -> Reconditioner {
        Reconditioner {
            loop_var: 0,
            wrappers: HashSet::new(),
            only_loops: options.only_loops,
            unstable_float: options.unstable_float,
        }
    }

    fn recondition_fn(&mut self, mut decl: FnDecl) -> FnDecl {
        decl.body = decl
            .body
            .into_iter()
            .map(|s| self.recondition_stmt(s))
            .collect();
        decl
    }

    fn recondition_else(&mut self, els: Else) -> Else {
        match els {
            Else::If(IfStatement {
                condition,
                body,
                else_,
            }) => Else::If(IfStatement {
                condition: self.recondition_expr(condition),
                body: body.into_iter().map(|s| self.recondition_stmt(s)).collect(),
                else_: else_.map(|els| Box::new(self.recondition_else(*els))),
            }),
            Else::Else(stmts) => Else::Else(
                stmts
                    .into_iter()
                    .map(|s| self.recondition_stmt(s))
                    .collect(),
            ),
        }
    }

    fn recondition_stmt(&mut self, stmt: Statement) -> Statement {
        match stmt {
            Statement::LetDecl(LetDeclStatement {
                ident,
                initializer,
                data_type,
            }) => {
                LetDeclStatement::new(ident, data_type, self.recondition_expr(initializer)).into()
            }
            Statement::ConstDecl(ConstDeclStatement {
                ident,
                initializer,
                data_type,
            }) => {
                ConstDeclStatement::new(ident, data_type, self.recondition_expr(initializer)).into()
            }
            Statement::VarDecl(VarDeclStatement {
                ident,
                data_type,
                initializer,
            }) => VarDeclStatement::new(
                ident,
                data_type,
                initializer.map(|e| self.recondition_expr(e)),
            )
            .into(),
            Statement::Assignment(AssignmentStatement { lhs, op, rhs }) => {
                AssignmentStatement::new(
                    self.recondition_assignment_lhs(lhs),
                    op,
                    self.recondition_expr(rhs),
                )
                .into()
            }
            Statement::Compound(s) => {
                Statement::Compound(s.into_iter().map(|s| self.recondition_stmt(s)).collect())
            }
            Statement::If(IfStatement {
                condition,
                body,
                else_,
            }) => IfStatement::new(
                self.recondition_expr(condition),
                body.into_iter().map(|s| self.recondition_stmt(s)).collect(),
            )
            .with_else(else_.map(|els| self.recondition_else(*els)))
            .into(),
            Statement::Return(ReturnStatement { value }) => ReturnStatement {
                value: value.map(|e| self.recondition_expr(e)),
            }
            .into(),
            Statement::Loop(LoopStatement {
                mut body,
                continuing,
            }) => {
                if continuing.is_some() {
                    let last_decl_idx = body.iter().rposition(|s| {
                        matches!(
                            s,
                            Statement::LetDecl(_) | Statement::VarDecl(_) | Statement::ConstDecl(_)
                        )
                    });
                    if let Some(idx) = last_decl_idx {
                        for stmt in body.iter_mut().take(idx) {
                            Self::replace_continue(stmt);
                        }
                    }
                }

                LoopStatement::new(
                    self.recondition_loop_body(body),
                    continuing.map(|ContinuingBlock { stmts, break_if }| {
                        let new_stmts: Vec<Statement> = stmts
                            .into_iter()
                            .map(|s| self.recondition_stmt(s))
                            .collect();

                        // // Naga incorrectly forbids shadowing declarations from loop body in continuing block
                        // if new_stmts.len() != 1 || !matches!(new_stmts[0], Statement::Compound(_)) {
                        //     new_stmts = vec![Statement::Compound(new_stmts)];
                        // }

                        ContinuingBlock {
                            stmts: new_stmts,
                            break_if: break_if.map(|e| self.recondition_expr(e)),
                        }
                    }),
                )
                .into()
            }
            Statement::While(stmt) => Statement::While(WhileStatement {
                condition: self.recondition_expr(stmt.condition),
                body: self.recondition_loop_body(stmt.body),
            }),
            Statement::Break => Statement::Break,
            Statement::Switch(SwitchStatement {
                selector,
                cases,
                default,
            }) => SwitchStatement::new(
                self.recondition_expr(selector),
                cases
                    .into_iter()
                    .map(|SwitchCase { selector, body }| SwitchCase {
                        selector: self.recondition_expr(selector),
                        body: body
                            .into_iter()
                            .map(|it| self.recondition_stmt(it))
                            .collect(),
                    })
                    .collect(),
                default
                    .into_iter()
                    .map(|it| self.recondition_stmt(it))
                    .collect(),
            )
            .into(),
            Statement::ForLoop(ForLoopStatement { header, body }) => ForLoopStatement::new(
                ForLoopHeader {
                    init: header.init.map(|init| self.recondition_for_init(init)),
                    condition: header.condition.map(|e| self.recondition_expr(e)),
                    update: header
                        .update
                        .map(|update| self.recondition_for_update(update)),
                },
                self.recondition_loop_body(body),
            )
            .into(),
            Statement::FnCall(FnCallStatement { ident, args }) => {
                Statement::FnCall(FnCallStatement::new(
                    ident,
                    args.into_iter()
                        .map(|it| self.recondition_expr(it))
                        .collect(),
                ))
            }
            Statement::Continue => Statement::Continue,

            Statement::Fallthrough => Statement::Fallthrough,
            Statement::Increment(IncrementStatement { lhs }) => {
                IncrementStatement::new(self.recondition_assignment_lhs(lhs)).into()
            }
            Statement::Decrement(DecrementStatement { lhs }) => {
                DecrementStatement::new(self.recondition_assignment_lhs(lhs)).into()
            }

            Statement::ConstAssert(s) => Statement::ConstAssert(ConstAssertStatement::new(
                self.recondition_expr(s.condition),
            )),
            Statement::Discard(s) => Statement::Discard(s),
        }
    }

    fn recondition_for_init(&mut self, init: ForLoopInit) -> ForLoopInit {
        match init {
            ForLoopInit::VarDecl(VarDeclStatement {
                ident,
                data_type,
                initializer,
            }) => ForLoopInit::VarDecl(VarDeclStatement::new(
                ident,
                data_type,
                initializer.map(|e| self.recondition_expr(e)),
            )),
            ForLoopInit::LetDecl(LetDeclStatement {
                ident,
                data_type,
                initializer,
            }) => ForLoopInit::LetDecl(LetDeclStatement::new(
                ident,
                data_type,
                self.recondition_expr(initializer),
            )),
            ForLoopInit::ConstDecl(ConstDeclStatement {
                ident,
                data_type,
                initializer,
            }) => ForLoopInit::ConstDecl(ConstDeclStatement::new(
                ident,
                data_type,
                self.recondition_expr(initializer),
            )),
            ForLoopInit::Assignment(AssignmentStatement { lhs, op, rhs }) => {
                ForLoopInit::Assignment(AssignmentStatement::new(
                    self.recondition_assignment_lhs(lhs),
                    op,
                    self.recondition_expr(rhs),
                ))
            }
            ForLoopInit::Increment(IncrementStatement { lhs }) => ForLoopInit::Increment(
                IncrementStatement::new(self.recondition_assignment_lhs(lhs)),
            ),
            ForLoopInit::Decrement(DecrementStatement { lhs }) => ForLoopInit::Decrement(
                DecrementStatement::new(self.recondition_assignment_lhs(lhs)),
            ),
            ForLoopInit::Call(FnCallStatement { ident, args }) => {
                ForLoopInit::Call(FnCallStatement::new(
                    ident,
                    args.into_iter()
                        .map(|it| self.recondition_expr(it))
                        .collect(),
                ))
            }
        }
    }

    fn recondition_for_update(&mut self, update: ForLoopUpdate) -> ForLoopUpdate {
        match update {
            ForLoopUpdate::Assignment(AssignmentStatement { lhs, op, rhs }) => {
                ForLoopUpdate::Assignment(AssignmentStatement::new(
                    self.recondition_assignment_lhs(lhs),
                    op,
                    self.recondition_expr(rhs),
                ))
            }
            ForLoopUpdate::Increment(IncrementStatement { lhs }) => ForLoopUpdate::Increment(
                IncrementStatement::new(self.recondition_assignment_lhs(lhs)),
            ),
            ForLoopUpdate::Decrement(DecrementStatement { lhs }) => ForLoopUpdate::Decrement(
                DecrementStatement::new(self.recondition_assignment_lhs(lhs)),
            ),
            ForLoopUpdate::Call(FnCallStatement { ident, args }) => {
                ForLoopUpdate::Call(FnCallStatement::new(
                    ident,
                    args.into_iter()
                        .map(|it| self.recondition_expr(it))
                        .collect(),
                ))
            }
        }
    }

    fn recondition_loop_body(&mut self, body: Vec<Statement>) -> Vec<Statement> {
        let id = self.loop_var();

        let counters_ty = DataType::Ref(MemoryViewType::new(
            DataType::array(ScalarType::U32, None),
            StorageClass::Private,
        ));

        let break_check = IfStatement::new(
            BinOpExpr::new(
                BinOp::GreaterEqual,
                PostfixExpr::new(
                    VarExpr::new("LOOP_COUNTERS").into_node(counters_ty.clone()),
                    Postfix::index(Lit::U32(id)),
                ),
                Lit::U32(1),
            ),
            vec![Statement::Break],
        );

        let counter_increment = AssignmentStatement::new(
            AssignmentLhs::array_index("LOOP_COUNTERS", counters_ty.clone(), Lit::U32(id).into()),
            AssignmentOp::Simple,
            BinOpExpr::new(
                BinOp::Plus,
                PostfixExpr::new(
                    VarExpr::new("LOOP_COUNTERS").into_node(counters_ty),
                    Postfix::index(Lit::U32(id)),
                ),
                Lit::U32(1),
            ),
        );

        std::iter::once(break_check.into())
            .chain(std::iter::once(counter_increment.into()))
            .chain(body.into_iter().map(|s| self.recondition_stmt(s)))
            .collect()
    }

    fn recondition_assignment_lhs(&mut self, lhs: AssignmentLhs) -> AssignmentLhs {
        if self.only_loops {
            return lhs;
        }

        match lhs {
            AssignmentLhs::Phony => AssignmentLhs::Phony,
            AssignmentLhs::Expr(expr) => AssignmentLhs::Expr(self.recondition_lhs_expr(expr)),
        }
    }

    fn recondition_lhs_expr(&mut self, node: LhsExprNode) -> LhsExprNode {
        let expr = match node.expr {
            LhsExpr::Ident(ident) => LhsExpr::Ident(ident),
            LhsExpr::Postfix(expr, postfix) => {
                let expr = Box::new(self.recondition_lhs_expr(*expr));
                let postfix = match postfix {
                    Postfix::Index(index) => {
                        let index = self.recondition_expr(*index);
                        let array_expr = match expr.data_type.dereference() {
                            DataType::Array(_, None) => Some(Self::lhs_to_expr(&expr)),
                            _ => None,
                        };
                        Postfix::index(self.recondition_index(&expr.data_type, array_expr, index))
                    }
                    Postfix::Member(ident) => Postfix::Member(ident),
                };

                LhsExpr::Postfix(expr, postfix)
            }
            LhsExpr::Deref(expr) => LhsExpr::Deref(Box::new(self.recondition_lhs_expr(*expr))),
            LhsExpr::AddressOf(expr) => {
                LhsExpr::AddressOf(Box::new(self.recondition_lhs_expr(*expr)))
            }
        };

        LhsExprNode { expr, ..node }
    }

    fn recondition_expr(&mut self, node: ExprNode) -> ExprNode {
        if self.only_loops {
            return node;
        }

        let reconditioned = match node.expr {
            Expr::TypeCons(expr) => Expr::TypeCons(TypeConsExpr::new(
                expr.data_type,
                expr.args
                    .into_iter()
                    .map(|e| self.recondition_expr(e))
                    .collect(),
            )),
            Expr::UnOp(expr) => {
                let inner = self.recondition_expr(*expr.inner);
                let op = expr.op;
                match op {
                    UnOp::Neg => {
                        let data_type = inner.data_type.dereference().clone();
                        let mut expr = UnOpExpr::new(UnOp::Neg, inner).into();
                        if !data_type.is_matrix() {
                            if let Some(scalar) = data_type.as_scalar() {
                                if matches!(scalar, ScalarType::F32 | ScalarType::F16) {
                                    expr = FnCallExpr::new(
                                        self.safe_wrapper(Wrapper::FloatOp(data_type.clone())),
                                        vec![ExprNode { data_type, expr }],
                                    )
                                    .into();
                                }
                            }
                        }
                        expr
                    }
                    _ => UnOpExpr::new(op, inner).into(),
                }
            }
            Expr::BinOp(expr) => {
                let left = self.recondition_expr(*expr.left);
                let right = self.recondition_expr(*expr.right);
                if self.unstable_float {
                    if let BinOp::LShift | BinOp::RShift = expr.op {
                        return self.recondition_shift_expr(node.data_type, expr.op, left, right);
                    }
                    let binop = ExprNode {
                        data_type: node.data_type.clone(),
                        expr: Expr::BinOp(BinOpExpr::new(expr.op, left, right)),
                    };
                    if !node.data_type.is_matrix() && matches!(node.data_type.as_scalar(), Some(ScalarType::F32 | ScalarType::F16)) {
                        return FnCallExpr::new(
                            self.safe_wrapper(Wrapper::FloatOp(node.data_type.clone())),
                            vec![binop],
                        )
                        .into_node(node.data_type);
                    }
                    return binop;
                }
                return self.recondition_bin_op_expr(node.data_type, expr.op, left, right);
            }
            Expr::FnCall(expr) => {
                let args: Vec<ExprNode> = expr
                    .args
                    .into_iter()
                    .map(|e| self.recondition_expr(e))
                    .collect();

                if self.unstable_float {
                    let mut new_call = FnCallExpr::new(expr.ident, args);
                    new_call.template_args = expr.template_args;
                    let call_node = ExprNode {
                        data_type: node.data_type.clone(),
                        expr: Expr::FnCall(new_call),
                    };
                    if !node.data_type.is_matrix() && matches!(node.data_type.as_scalar(), Some(ScalarType::F32 | ScalarType::F16)) {
                        return FnCallExpr::new(
                            self.safe_wrapper(Wrapper::FloatOp(node.data_type.clone())),
                            vec![call_node],
                        )
                        .into_node(node.data_type);
                    }
                    return call_node;
                }

                let expr = match expr.ident.as_str() {
                    "extractBits" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::ExtractBits(
                            args[0].data_type.dereference().clone(),
                        )),
                        args,
                    ),
                    "insertBits" if args[0].data_type.is_integer() => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::InsertBits(
                            args[0].data_type.dereference().clone(),
                        )),
                        args,
                    ),
                    "select" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Select(
                            args[0].data_type.dereference().clone(),
                            args[2].data_type.dereference().clone(),
                        )),
                        args,
                    ),
                    "pack2x16float" => {
                        FnCallExpr::new(self.safe_wrapper(Wrapper::Pack2x16float), args)
                    }
                    "smoothstep" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Smoothstep(
                            args[0].data_type.dereference().clone(),
                        )),
                        args,
                    ),
                    "normalize" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Normalize(
                            args[0].data_type.dereference().clone(),
                        )),
                        args,
                    ),
                    "acos" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Acos(args[0].data_type.dereference().clone())),
                        args,
                    ),
                    "acosh" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Acosh(args[0].data_type.dereference().clone())),
                        args,
                    ),
                    "asin" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Asin(args[0].data_type.dereference().clone())),
                        args,
                    ),
                    "atanh" => FnCallExpr::new(
                        self.safe_wrapper(Wrapper::Atanh(args[0].data_type.dereference().clone())),
                        args,
                    ),
                    "subgroupBroadcast"
                    | "subgroupShuffle"
                    | "subgroupShuffleDown"
                    | "subgroupShuffleUp"
                    | "subgroupShuffleXor" => {
                        let mut safe_args = args.clone();
                        let limit = 127;
                        let limit_lit = match safe_args[1].data_type.as_scalar().unwrap() {
                            ScalarType::I32 => Lit::I32(limit),
                            ScalarType::U32 => Lit::U32(limit as u32),
                            _ => unreachable!(),
                        };
                        safe_args[1] =
                            BinOpExpr::new(BinOp::BitAnd, safe_args[1].clone(), limit_lit).into();
                        let mut new_call = FnCallExpr::new(expr.ident.clone(), safe_args);
                        new_call.template_args = expr.template_args;
                        new_call
                    }
                    "quadBroadcast" => {
                        let mut safe_args = args.clone();
                        let limit = 3;
                        let limit_lit = match safe_args[1].data_type.as_scalar().unwrap() {
                            ScalarType::I32 => Lit::I32(limit),
                            ScalarType::U32 => Lit::U32(limit as u32),
                            _ => unreachable!(),
                        };
                        safe_args[1] =
                            BinOpExpr::new(BinOp::BitAnd, safe_args[1].clone(), limit_lit).into();
                        let mut new_call = FnCallExpr::new(expr.ident.clone(), safe_args);
                        new_call.template_args = expr.template_args;
                        new_call
                    }
                    _ => {
                        let mut new_call = FnCallExpr::new(expr.ident, args);
                        new_call.template_args = expr.template_args;
                        new_call
                    }
                };

                if !node.data_type.is_matrix()
                    && matches!(
                        node.data_type.as_scalar(),
                        Some(ScalarType::F32 | ScalarType::F16)
                    )
                {
                    FnCallExpr::new(
                        self.safe_wrapper(Wrapper::FloatOp(node.data_type.clone())),
                        vec![expr.into_node(node.data_type.clone())],
                    )
                    .into()
                } else {
                    expr.into()
                }
            }
            Expr::Postfix(expr) => {
                let e = self.recondition_expr(*expr.inner);
                let postfix = match expr.postfix {
                    Postfix::Index(index) => {
                        let index = self.recondition_expr(*index);
                        let array_expr = match e.data_type.dereference() {
                            DataType::Array(_, None) => Some(e.clone()),
                            _ => None,
                        };
                        Postfix::Index(Box::new(self.recondition_index(
                            &e.data_type,
                            array_expr,
                            index,
                        )))
                    }
                    Postfix::Member(n) => Postfix::Member(n),
                };

                PostfixExpr::new(e, postfix).into()
            }
            e => e,
        };

        ExprNode {
            data_type: node.data_type,
            expr: reconditioned,
        }
    }

    fn lhs_to_expr(node: &LhsExprNode) -> ExprNode {
        let expr = match &node.expr {
            LhsExpr::Ident(ident) => Expr::Var(VarExpr::new(ident.clone())),
            LhsExpr::Postfix(inner, postfix) => {
                Expr::Postfix(PostfixExpr::new(Self::lhs_to_expr(inner), postfix.clone()))
            }
            LhsExpr::Deref(inner) => {
                Expr::UnOp(UnOpExpr::new(UnOp::Deref, Self::lhs_to_expr(inner)))
            }
            LhsExpr::AddressOf(inner) => {
                Expr::UnOp(UnOpExpr::new(UnOp::AddressOf, Self::lhs_to_expr(inner)))
            }
        };

        ExprNode {
            data_type: node.data_type.clone(),
            expr,
        }
    }

    fn recondition_index(
        &mut self,
        array_type: &DataType,
        array_expr: Option<ExprNode>,
        index: ExprNode,
    ) -> ExprNode {
        let size = match array_type.dereference() {
            DataType::Array(_, Some(n)) => Some(*n),
            DataType::Array(_, None) => None,
            DataType::Vector(n, _) => Some(*n as u32),
            DataType::Matrix(c, _, _) => Some(*c as u32),
            _ => unreachable!("index operator cannot be applied to type `{array_type}`"),
        };

        let index_type = index.data_type.dereference().clone();
        let size_expr = match size {
            Some(n) => match index_type.as_scalar().unwrap() {
                ScalarType::I32 => Lit::I32(n as i32).into(),
                ScalarType::U32 => Lit::U32(n).into(),
                _ => unreachable!("index expression must be an integer"),
            },
            None => {
                let array_expr = array_expr.expect("runtime array needs expression");
                let addr = UnOpExpr::new(UnOp::AddressOf, array_expr).into();
                match index.data_type.as_scalar().unwrap() {
                    ScalarType::I32 => TypeConsExpr::new(
                        ScalarType::I32.into(),
                        vec![FnCallExpr::new("arrayLength", vec![addr]).into_node(ScalarType::U32)],
                    )
                    .into(),
                    ScalarType::U32 => {
                        FnCallExpr::new("arrayLength", vec![addr]).into_node(ScalarType::U32)
                    }
                    _ => unreachable!("index expression must be an integer"),
                }
            }
        };

        FnCallExpr::new(
            self.safe_wrapper(Wrapper::Index(index_type.clone())),
            vec![index, size_expr],
        )
        .into_node(index_type)
    }

    fn recondition_shift_expr(
        &mut self,
        ty: DataType,
        shift_op: BinOp,
        operand: ExprNode,
        shift_value: ExprNode,
    ) -> ExprNode {
        let shift_type = shift_value.data_type.dereference();
        let shift_bound: ExprNode = match ty {
            DataType::Scalar(_) => Lit::U32(32).into(),
            DataType::Vector(_, _) => {
                TypeConsExpr::new(shift_type.clone(), vec![Lit::U32(32).into()]).into()
            }
            _ => unreachable!(),
        };

        ExprNode::from(BinOpExpr::new(
            shift_op,
            operand,
            BinOpExpr::new(BinOp::Mod, shift_value, shift_bound),
        ))
    }

    fn recondition_bin_op_expr(
        &mut self,
        data_type: DataType,
        op: BinOp,
        l: ExprNode,
        r: ExprNode,
    ) -> ExprNode {
        if data_type.is_matrix() {
            return BinOpExpr::new(op, l, r).into();
        }

        if let BinOp::LShift | BinOp::RShift = op {
            return self.recondition_shift_expr(data_type, op, l, r);
        }

        match data_type.as_scalar().unwrap() {
            ScalarType::I32 | ScalarType::U32 => {
                self.recondition_integer_bin_op_expr(data_type, op, l, r)
            }
            ScalarType::F32 | ScalarType::F16 if op == BinOp::Divide => {
                self.recondition_floating_point_div_expr(data_type, op, l, r)
            }
            ScalarType::F32 | ScalarType::F16 => {
                self.recondition_floating_point_bin_op_expr(data_type, op, l, r)
            }
            ScalarType::Bool => BinOpExpr::new(op, l, r).into(),
        }
    }

    fn splat_scalar_to_vector(data_type: &DataType, mut expr: ExprNode) -> ExprNode {
        if let DataType::Vector(_, _) = data_type {
            if expr.data_type.dereference().is_scalar() {
                expr = ExprNode {
                    data_type: data_type.clone(),
                    expr: Expr::TypeCons(TypeConsExpr::new(data_type.clone(), vec![expr])),
                };
            }
        }
        expr
    }

    fn recondition_integer_bin_op_expr(
        &mut self,
        data_type: DataType,
        op: BinOp,
        l: ExprNode,
        r: ExprNode,
    ) -> ExprNode {
        let name = match op {
            BinOp::Mod => self.safe_wrapper(Wrapper::Mod(data_type.clone())),
            op => return BinOpExpr::new(op, l, r).into(),
        };

        let l = Self::splat_scalar_to_vector(&data_type, l);
        let r = Self::splat_scalar_to_vector(&data_type, r);

        FnCallExpr::new(name, vec![l, r]).into_node(data_type)
    }

    fn recondition_floating_point_bin_op_expr(
        &mut self,
        data_type: DataType,
        op: BinOp,
        l: ExprNode,
        r: ExprNode,
    ) -> ExprNode {
        FnCallExpr::new(
            self.safe_wrapper(Wrapper::FloatOp(data_type.clone())),
            vec![BinOpExpr::new(op, l, r).into()],
        )
        .into_node(data_type)
    }

    fn recondition_floating_point_div_expr(
        &mut self,
        data_type: DataType,
        op: BinOp,
        l: ExprNode,
        r: ExprNode,
    ) -> ExprNode {
        let wrapper = match op {
            BinOp::Divide => Wrapper::FloatDivide(data_type.clone()),
            _ => unreachable!(),
        };

        let l = Self::splat_scalar_to_vector(&data_type, l);
        let r = Self::splat_scalar_to_vector(&data_type, r);

        FnCallExpr::new(self.safe_wrapper(wrapper), vec![l, r]).into_node(data_type)
    }

    fn loop_var(&mut self) -> u32 {
        let cur = self.loop_var;
        self.loop_var += 1;
        cur
    }

    fn safe_wrapper(&mut self, wrapper: Wrapper) -> String {
        let ident = wrapper.to_string();
        self.wrappers.insert(wrapper);
        ident
    }
    fn replace_continue(stmt: &mut Statement) {
        match stmt {
            Statement::Continue => {
                *stmt = Statement::Break;
            }
            Statement::Compound(stmts) => {
                for s in stmts {
                    Self::replace_continue(s);
                }
            }
            Statement::If(stmt) => {
                for s in &mut stmt.body {
                    Self::replace_continue(s);
                }
                if let Some(else_) = &mut stmt.else_ {
                    Self::replace_continue_else(else_.as_mut());
                }
            }
            Statement::Switch(stmt) => {
                for case in &mut stmt.cases {
                    for s in &mut case.body {
                        Self::replace_continue(s);
                    }
                }
                for s in &mut stmt.default {
                    Self::replace_continue(s);
                }
            }
            _ => {}
        }
    }

    fn replace_continue_else(else_: &mut Else) {
        match else_ {
            Else::If(stmt) => {
                for s in &mut stmt.body {
                    Self::replace_continue(s);
                }
                if let Some(els) = &mut stmt.else_ {
                    Self::replace_continue_else(els.as_mut());
                }
            }
            Else::Else(stmts) => {
                for s in stmts {
                    Self::replace_continue(s);
                }
            }
        }
    }
}
