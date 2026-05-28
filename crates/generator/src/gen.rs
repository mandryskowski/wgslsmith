mod cx;
mod expr;
mod fns;
mod hash;
mod scope;
mod stmt;
mod structs;
mod utils;

pub mod builtins;

use std::rc::Rc;

use ast::types::{DataType, MemoryViewType};
use ast::{
    AccessMode, AssignmentLhs, AssignmentOp, AssignmentStatement, BuiltinValue, ConstDeclStatement, Expr, ExprNode,
    FnAttr, FnDecl, FnInput, FnParamReturnAttr, GlobalVarAttr, GlobalVarDecl, LetDeclStatement,
    Lit, Module, OverrideDecl, Postfix, PostfixExpr, ReturnStatement, ScalarType, Statement, StorageClass,
    VarExpr, VarQualifier,
};
use rand::prelude::{SliceRandom, StdRng};
use rand::Rng;
use rand_distr::{Binomial, Distribution, StandardNormal};

use crate::gen::scope::Scope;
use crate::Options;

use self::cx::Context;
use self::structs::StructKind;

#[derive(Default)]
struct FnState {
    is_loop: bool,
    is_continuing: bool,
    block_depth: u32,
    expression_depth: u32,
    is_entrypoint: bool,
    is_non_uniform: bool,
}

pub struct Generator<'a> {
    rng: &'a mut StdRng,
    options: Rc<Options>,
    cx: Context,
    return_type: Option<DataType>,
    fn_state: FnState,
    global_scope: Scope,
    scope: Scope,
    current_block: Vec<Statement>,
    f32_dist: StandardNormal,
    i32_dist: Binomial,
    u32_dist: Binomial,
}

impl<'a> Generator<'a> {
    pub fn with_context(mut self, module: &ast::Module) -> Self {
        for s in &module.structs {
            self.cx.types.insert(s.clone());
            self.cx.types.mark_imported(&s.name);
        }
        for f in &module.functions {
            let is_entry = f.attrs.iter().any(|a| matches!(a, ast::FnAttr::Stage(_)));
            let can_call = f
                .inputs
                .iter()
                .all(|p| p.data_type.dereference().is_constructible())
                && f.output
                    .as_ref()
                    .is_none_or(|out| out.data_type.dereference().is_constructible());

            if !is_entry && can_call {
                self.cx.fns.insert(f.clone());
                self.cx.fns.mark_imported(&f.name);
            }
        }
        self
    }

    pub fn new(rng: &'a mut StdRng, options: Rc<Options>) -> Self {
        Generator {
            rng,
            options: options.clone(),
            cx: Context::new(options),
            return_type: None,
            fn_state: FnState::default(),
            global_scope: Scope::empty(),
            scope: Scope::empty(),
            current_block: vec![],
            f32_dist: StandardNormal,
            i32_dist: Binomial::new(i32::MAX as u64 * 2, 0.5)
                .expect("failed to create binomial distribution"),
            u32_dist: Binomial::new(u32::MAX as u64 * 2, 0.5)
                .expect("failed to create binomial distribution"),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn gen_module(&mut self) -> Module {
        let struct_count = self
            .rng
            .gen_range(self.options.min_structs..=self.options.max_structs);

        for i in 1..=struct_count {
            let name = format!("Struct_{}", i);
            let decl = self.gen_struct(name);
            self.cx.types.insert(decl);
        }

        let ub_type_decl =
            self.gen_struct_with("UniformBuffer".to_owned(), StructKind::UniformBuffer);
        let sb_type_decl =
            self.gen_struct_with("StorageBuffer".to_owned(), StructKind::HostShareable);

        self.global_scope
            .insert_readonly("u_input".to_owned(), DataType::Struct(ub_type_decl.clone()));

        let can_write_storage = matches!(
            self.options.stage,
            crate::ShaderStage::Compute | crate::ShaderStage::Fragment
        );
        let storage_access = if can_write_storage {
            AccessMode::ReadWrite
        } else {
            AccessMode::Read
        };

        let mut global_vars = vec![
            GlobalVarDecl {
                attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(0)],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::Uniform,
                    access_mode: None,
                }),
                name: "u_input".to_owned(),
                data_type: DataType::Struct(ub_type_decl.clone()),
                initializer: None,
            },
            GlobalVarDecl {
                attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(1)],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::Storage,
                    access_mode: Some(storage_access),
                }),
                name: "s_output".to_owned(),
                data_type: DataType::Struct(sb_type_decl.clone()),
                initializer: None,
            },
            GlobalVarDecl {
                attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(2)],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::Storage,
                    access_mode: Some(storage_access),
                }),
                name: "hash_output".to_owned(),
                data_type: DataType::Scalar(ScalarType::U32),
                initializer: None,
            },
        ];

        let num_extra_sbs = self.rng.gen_range(1..=3);
        let mut extra_sb_decls = Vec::new();

        for i in 0..num_extra_sbs {
            let extra_sb_decl =
                self.gen_struct_with(format!("StorageBuffer_{i}"), StructKind::HostShareable);
            extra_sb_decls.push(extra_sb_decl.clone());

            let name = format!("s_buf_{i}");
            let data_type = DataType::Struct(extra_sb_decl.clone());

            global_vars.push(GlobalVarDecl {
                attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(3 + i)],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::Storage,
                    access_mode: Some(storage_access),
                }),
                name: name.clone(),
                data_type: data_type.clone(),
                initializer: None,
            });

            let ref_type = DataType::Ref(MemoryViewType {
                inner: Rc::new(data_type.clone()),
                storage_class: StorageClass::Storage,
                access_mode: storage_access,
            });

            if can_write_storage {
                self.global_scope.insert_mutable(name, ref_type);
            } else {
                self.global_scope
                    .insert_unassignable_reference(name, ref_type);
            }
        }

        let mut workgroup_size = 0;

        for i in 0..self.rng.gen_range(0..=5) {
            let name = format!("global{i}");
            global_vars.push(self.gen_global_var(name, &mut workgroup_size));
        }

        let mut consts = vec![];
        for i in 0..self.rng.gen_range(0..=5) {
            let ty = self.cx.types.select(self.rng);
            let name = format!("global_const_{i}");
            let initializer = self.gen_const_expr(&ty);
            consts.push(ConstDeclStatement::new(name.clone(), Some(ty.clone()), initializer));
            self.global_scope.insert_readonly(name, ty);
        }

        let mut overrides = vec![];
        let mut allowed_override_scalars = vec![
            ScalarType::I32,
            ScalarType::U32,
            ScalarType::F32,
            ScalarType::Bool,
        ];
        if self.options.enable_f16() {
            allowed_override_scalars.push(ScalarType::F16);
        }
        for i in 0..self.rng.gen_range(0..=0) {
            let ty = DataType::Scalar(*allowed_override_scalars.choose(self.rng).unwrap());
            let name = format!("override_{i}");
            let initializer = Some(self.gen_const_expr(&ty));
            overrides.push(OverrideDecl {
                attrs: vec![],
                name: name.clone(),
                data_type: Some(ty.clone()),
                initializer,
            });
            self.global_scope.insert_readonly(name, ty);
        }

        if self.options.stage == crate::ShaderStage::Compute {
            let atomic_vars = [
                ("wg_atomic_u32", DataType::Atomic(ScalarType::U32)),
                ("wg_atomic_i32", DataType::Atomic(ScalarType::I32)),
            ];
            for (name, ty) in atomic_vars {
                global_vars.push(GlobalVarDecl {
                    attrs: vec![],
                    qualifier: Some(VarQualifier {
                        storage_class: StorageClass::WorkGroup,
                        access_mode: None,
                    }),
                    name: name.to_owned(),
                    data_type: ty.clone(),
                    initializer: None,
                });
                self.global_scope.insert_unassignable_reference(
                    name.to_owned(),
                    DataType::Ref(MemoryViewType::new(ty.clone(), StorageClass::WorkGroup)),
                );
            }

            if self.options.collectives() {
                let mut wg_vars = vec![
                    ("wg_u32", DataType::Scalar(ScalarType::U32)),
                    ("wg_i32", DataType::Scalar(ScalarType::I32)),
                    ("wg_f32", DataType::Scalar(ScalarType::F32)),
                    ("wg_bool", DataType::Scalar(ScalarType::Bool)),
                ];
                if self.options.enable_f16() {
                    wg_vars.push(("wg_f16", DataType::Scalar(ScalarType::F16)));
                }
                for (name, ty) in wg_vars {
                    global_vars.push(GlobalVarDecl {
                        attrs: vec![],
                        qualifier: Some(VarQualifier {
                            storage_class: StorageClass::WorkGroup,
                            access_mode: None,
                        }),
                        name: name.to_owned(),
                        data_type: ty.clone(),
                        initializer: None,
                    });
                    self.global_scope.insert_mutable(
                        name.to_owned(),
                        DataType::Ref(MemoryViewType::new(ty.clone(), StorageClass::WorkGroup)),
                    );
                }
            }
        }

        let entrypoint = self.gen_entrypoint_function(
            DataType::Struct(ub_type_decl.clone()),
            DataType::Struct(sb_type_decl.clone()),
        );

        let Context { types, fns } =
            std::mem::replace(&mut self.cx, Context::new(self.options.clone()));

        let mut functions = fns.into_fns();

        functions.push(entrypoint);

        let mut enables = vec![];
        for ext in &self.options.extensions {
            let enable = ast::EnableExtension::from(*ext);
            if !enables.contains(&enable) {
                enables.push(enable);
            }
        }

        Module {
            enables,
            requires: vec![],
            aliases: vec![],
            structs: {
                let mut structs = types.into_structs();
                structs.push(ub_type_decl);
                structs.push(sb_type_decl);
                structs.extend(extra_sb_decls);
                structs
            },
            consts,
            const_asserts: vec![],
            overrides,
            vars: global_vars,
            functions,
        }
    }

    fn gen_global_var(&mut self, name: String, workgroup_size: &mut u32) -> GlobalVarDecl {
        let mut data_type;
        let mut storage_class;

        data_type = self.cx.types.select(self.rng);

        if self.rng.gen_bool(0.5) {
            data_type = DataType::Array(Rc::new(data_type), Some(self.rng.gen_range(1..=32)));
        }

        storage_class =
            if self.options.stage == crate::ShaderStage::Compute && self.rng.gen_bool(0.5) {
                StorageClass::WorkGroup
            } else {
                StorageClass::Private
            };

        // If the global variable won't fit in the workgroup storage, fallback to private
        if storage_class == StorageClass::WorkGroup {
            let align = structs::align_of(&data_type);
            let size = structs::size_of(&data_type);

            let offset = structs::round_up(align, *workgroup_size);

            if offset + size <= self.options.max_compute_workgroup_storage_size {
                *workgroup_size = offset + size;
            } else {
                storage_class = StorageClass::Private;
            }
        }

        let mem_view = MemoryViewType::new(data_type.clone(), storage_class);
        let ref_type = DataType::Ref(mem_view);

        self.global_scope.insert_mutable(name.clone(), ref_type);

        let initializer = if storage_class == StorageClass::Private && self.rng.gen_bool(0.75) {
            Some(self.gen_const_expr(&data_type))
        } else {
            None
        };

        GlobalVarDecl {
            attrs: vec![],
            qualifier: Some(VarQualifier {
                storage_class,
                access_mode: None,
            }),
            name,
            data_type,
            initializer,
        }
    }

    #[tracing::instrument(skip(self))]
    fn gen_entrypoint_function(&mut self, in_buf_type: DataType, out_buf_type: DataType) -> FnDecl {
        let prev_is_entrypoint = std::mem::replace(&mut self.fn_state.is_entrypoint, true);

        let mut function_scope = self.global_scope.clone();
        let mut inputs = vec![];

        let mut available_builtins = match self.options.stage {
            crate::ShaderStage::Compute => {
                let mut b = vec![
                    (
                        BuiltinValue::LocalInvocationId,
                        DataType::Vector(3, ScalarType::U32),
                    ),
                    (
                        BuiltinValue::LocalInvocationIndex,
                        DataType::Scalar(ScalarType::U32),
                    ),
                    (
                        BuiltinValue::GlobalInvocationId,
                        DataType::Vector(3, ScalarType::U32),
                    ),
                    (
                        BuiltinValue::WorkgroupId,
                        DataType::Vector(3, ScalarType::U32),
                    ),
                    (
                        BuiltinValue::NumWorkgroups,
                        DataType::Vector(3, ScalarType::U32),
                    ),
                ];

                if self.options.collectives() {
                    b.push((
                        BuiltinValue::SubgroupInvocationId,
                        DataType::Scalar(ScalarType::U32),
                    ));
                    b.push((
                        BuiltinValue::SubgroupSize,
                        DataType::Scalar(ScalarType::U32),
                    ));
                }
                b
            }
            crate::ShaderStage::Vertex => {
                vec![
                    (BuiltinValue::VertexIndex, DataType::Scalar(ScalarType::U32)),
                    (
                        BuiltinValue::InstanceIndex,
                        DataType::Scalar(ScalarType::U32),
                    ),
                ]
            }
            crate::ShaderStage::Fragment => {
                vec![
                    (BuiltinValue::Position, DataType::Vector(4, ScalarType::F32)),
                    (
                        BuiltinValue::FrontFacing,
                        DataType::Scalar(ScalarType::Bool),
                    ),
                    (BuiltinValue::SampleIndex, DataType::Scalar(ScalarType::U32)),
                    (BuiltinValue::SampleMask, DataType::Scalar(ScalarType::U32)),
                ]
            }
        };

        let num_params = self.rng.gen_range(0..=available_builtins.len());
        available_builtins.shuffle(self.rng);

        for (builtin, data_type) in available_builtins.into_iter().take(num_params) {
            let name = format!("gl_{}", builtin);
            inputs.push(FnInput {
                attrs: vec![FnParamReturnAttr::Builtin(builtin)],
                name: name.clone(),
                data_type: data_type.clone(),
            });
            function_scope.insert_readonly(name, data_type);
        }

        let (output, ret_type) = match self.options.stage {
            crate::ShaderStage::Compute => (None, None),
            crate::ShaderStage::Vertex => (
                Some(ast::FnOutput {
                    attrs: vec![FnParamReturnAttr::Builtin(BuiltinValue::Position)],
                    data_type: DataType::Vector(4, ScalarType::F32),
                }),
                Some(DataType::Vector(4, ScalarType::F32)),
            ),
            crate::ShaderStage::Fragment => (
                Some(ast::FnOutput {
                    attrs: vec![FnParamReturnAttr::Location(0)],
                    data_type: DataType::Vector(4, ScalarType::F32),
                }),
                Some(DataType::Vector(4, ScalarType::F32)),
            ),
        };

        self.return_type = ret_type.clone();

        let stmt_count = self.rng.gen_range(5..10);
        let (_, block) = self.with_scope(function_scope, |this| {
            let (scope, mut block) = this.gen_stmt_block(stmt_count);

            if let Some(Statement::Return(_)) = block.last() {
                block.pop();
            }

            let prev_block = std::mem::replace(&mut this.current_block, block);

            this.with_scope(scope, |this| {
                this.current_block.push(
                    LetDeclStatement::new(
                        "x",
                        None,
                        PostfixExpr::new(
                            VarExpr::new("u_input").into_node(in_buf_type),
                            Postfix::member("a"),
                        ),
                    )
                    .into(),
                );

                let can_write_storage = matches!(
                    this.options.stage,
                    crate::ShaderStage::Compute | crate::ShaderStage::Fragment
                );

                if can_write_storage {
                    let out_rhs = this.gen_expr(&out_buf_type);
                    let out_lhs = AssignmentLhs::name("s_output", out_buf_type.clone());

                    this.current_block.push(
                        AssignmentStatement::new(out_lhs, AssignmentOp::Simple, out_rhs).into(),
                    );

                    if this.rng.gen_bool(0.5) {
                        let hash_val = this.gen_scope_hash_expr(&this.scope.clone());
                        let hash_lhs = AssignmentLhs::name("hash_output", ScalarType::U32);

                        this.current_block.push(
                            AssignmentStatement::new(hash_lhs, AssignmentOp::Simple, hash_val)
                                .into(),
                        );
                    }
                }

                if let Some(ret_ty) = &ret_type {
                    let ret_val = this.gen_expr(ret_ty);
                    this.current_block
                        .push(Statement::Return(ReturnStatement::new(ret_val)));
                }
            });

            std::mem::replace(&mut this.current_block, prev_block)
        });

        self.fn_state.is_entrypoint = prev_is_entrypoint;
        self.return_type = None;

        let mut attrs = vec![FnAttr::Stage(self.options.stage.into())];
        if self.options.stage == crate::ShaderStage::Compute {
            attrs.push(FnAttr::WorkgroupSize(vec![ExprNode {
                data_type: DataType::Scalar(ScalarType::U32),
                expr: Expr::Lit(Lit::U32(1)),
            }]));
        }

        FnDecl {
            attrs,
            name: "main".to_owned(),
            inputs,
            output,
            body: block,
        }
    }

    fn with_scope<T>(&mut self, scope: Scope, block: impl FnOnce(&mut Self) -> T) -> (Scope, T) {
        let old_scope = std::mem::replace(&mut self.scope, scope);
        let res = block(self);
        (std::mem::replace(&mut self.scope, old_scope), res)
    }

    fn with_non_uniform<T>(&mut self, block: impl FnOnce(&mut Self) -> T) -> T {
        let prev = std::mem::replace(&mut self.fn_state.is_non_uniform, true);
        let res = block(self);
        self.fn_state.is_non_uniform = prev;
        res
    }

    fn gen_i32(&mut self) -> i32 {
        if self.rng.gen_bool(0.5) {
            (self.i32_dist.sample(self.rng) as i64 - i32::MAX as i64) as i32
        } else {
            *[0, 1, -1, i32::MAX, i32::MIN].choose(self.rng).unwrap()
        }
    }

    fn gen_u32(&mut self) -> u32 {
        if self.rng.gen_bool(0.5) {
            (self.u32_dist.sample(self.rng) as i64 - u32::MAX as i64).unsigned_abs() as u32
        } else {
            *[0, 1, u32::MAX].choose(self.rng).unwrap()
        }
    }

    fn gen_f32(&mut self) -> f32 {
        let k: f64 = self.f32_dist.sample(&mut self.rng);
        let k = if k.abs() < 0.1 {
            if self.rng.gen_bool(0.5) {
                1.0
            } else {
                -1.0
            }
        } else {
            k
        };
        let x = k * 1000.0;
        f32::clamp(x.trunc() as f32, -16777216.0, 16777216.0)
    }
}
