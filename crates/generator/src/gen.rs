mod cx;
mod divergence;
mod expr;
mod fns;
mod hash;
mod scope;
mod stmt;
mod structs;
mod utils;

pub mod builtins;

use divergence::CopyInOut;

use std::rc::Rc;

use ast::types::{DataType, MemoryViewType};
use ast::{
    AccessMode, AssignmentLhs, AssignmentOp, AssignmentStatement, BuiltinValue, Expr, ExprNode,
    FnAttr, FnDecl, FnInput, FnParamReturnAttr, GlobalVarAttr, GlobalVarDecl, LetDeclStatement,
    LhsExprNode, Lit, Module, Postfix, PostfixExpr, ScalarType, ShaderStage, Statement,
    StorageClass, VarExpr, VarQualifier,
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
    wg_size: u32,
    copy_in_out: Vec<CopyInOut>,
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
        let wg_size = if options.enable_divergence {
            rng.gen_range(2..=32)
        } else {
            1
        };

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
            wg_size,
            copy_in_out: Vec::new(),
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

        let mut out_buf_global_type = DataType::Struct(sb_type_decl.clone());
        let mut hash_out_global_type = DataType::Scalar(ScalarType::U32);

        if self.wg_size > 1 {
            out_buf_global_type =
                DataType::Array(Rc::new(out_buf_global_type.clone()), Some(self.wg_size));

            hash_out_global_type =
                DataType::Array(Rc::new(hash_out_global_type.clone()), Some(self.wg_size));
        }

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
                    access_mode: Some(AccessMode::ReadWrite),
                }),
                name: "s_output".to_owned(),
                data_type: out_buf_global_type.clone(),
                initializer: None,
            },
            GlobalVarDecl {
                attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(2)],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::Storage,
                    access_mode: Some(AccessMode::ReadWrite),
                }),
                name: "hash_output".to_owned(),
                data_type: hash_out_global_type.clone(),
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

            if self.wg_size > 1 {
                let global_name = format!("{name}_global");
                let array_type = DataType::Array(Rc::new(data_type.clone()), Some(self.wg_size));

                global_vars.push(GlobalVarDecl {
                    attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(3 + i)],
                    qualifier: Some(VarQualifier {
                        storage_class: StorageClass::Storage,
                        access_mode: Some(AccessMode::ReadWrite),
                    }),
                    name: global_name.clone(),
                    data_type: array_type.clone(),
                    initializer: None,
                });

                global_vars.push(GlobalVarDecl {
                    attrs: vec![],
                    qualifier: Some(VarQualifier {
                        storage_class: StorageClass::Private,
                        access_mode: None,
                    }),
                    name: name.clone(),
                    data_type: data_type.clone(),
                    initializer: None,
                });

                self.global_scope.insert_mutable(
                    name.clone(),
                    DataType::Ref(MemoryViewType::new(
                        data_type.clone(),
                        StorageClass::Private,
                    )),
                );

                self.copy_in_out.push(CopyInOut {
                    name,
                    global_name,
                    data_type,
                    wrapper_type: array_type,
                    storage_class: StorageClass::Storage,
                });
            } else {
                global_vars.push(GlobalVarDecl {
                    attrs: vec![GlobalVarAttr::Group(0), GlobalVarAttr::Binding(3 + i)],
                    qualifier: Some(VarQualifier {
                        storage_class: StorageClass::Storage,
                        access_mode: Some(AccessMode::ReadWrite),
                    }),
                    name: name.clone(),
                    data_type: data_type.clone(),
                    initializer: None,
                });

                self.global_scope.insert_mutable(
                    name,
                    DataType::Ref(MemoryViewType {
                        inner: Rc::new(data_type),
                        storage_class: StorageClass::Storage,
                        access_mode: AccessMode::ReadWrite,
                    }),
                );
            }
        }

        let mut workgroup_size = 0;

        for i in 0..self.rng.gen_range(0..=5) {
            let name = format!("global{i}");
            global_vars.extend(self.gen_global_var(name, &mut workgroup_size));
        }

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

        if self.options.collectives {
            let wg_vars = [
                ("wg_u32", DataType::Scalar(ScalarType::U32)),
                ("wg_i32", DataType::Scalar(ScalarType::I32)),
                ("wg_f32", DataType::Scalar(ScalarType::F32)),
                ("wg_bool", DataType::Scalar(ScalarType::Bool)),
            ];
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

        if self.wg_size > 1 {
            let divergent_vars = divergence::generate_divergent_globals(
                &mut self.global_scope,
                self.options.enable_f16,
            );
            global_vars.extend(divergent_vars);
        }

        let entrypoint = self.gen_entrypoint_function(
            DataType::Struct(ub_type_decl.clone()),
            DataType::Struct(sb_type_decl.clone()),
            out_buf_global_type,
            hash_out_global_type,
        );

        let Context { types, fns } =
            std::mem::replace(&mut self.cx, Context::new(self.options.clone()));

        let mut functions = fns.into_fns();

        functions.push(entrypoint);

        let mut enables = vec![];
        if self.options.enable_f16 {
            enables.push(ast::EnableExtension::F16);
        }
        if self.options.collectives {
            enables.push(ast::EnableExtension::Subgroups);
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
            consts: vec![],
            const_asserts: vec![],
            overrides: vec![],
            vars: global_vars,
            functions,
        }
    }

    fn gen_global_var(&mut self, name: String, workgroup_size: &mut u32) -> Vec<GlobalVarDecl> {
        let mut data_type;
        let mut storage_class;

        data_type = self.cx.types.select(self.rng);

        if self.rng.gen_bool(0.5) {
            data_type = DataType::Array(Rc::new(data_type), Some(self.rng.gen_range(1..=32)));
        }

        storage_class = if self.rng.gen_bool(0.5) {
            StorageClass::WorkGroup
        } else {
            StorageClass::Private
        };

        // If the global variable won't fit in the workgroup storage, fallback to private
        if storage_class == StorageClass::WorkGroup {
            let align = structs::align_of(&data_type);
            let size = structs::size_of(&data_type);

            let stride = structs::round_up(align, size);
            let total_size = stride * self.wg_size;

            let offset = structs::round_up(align, *workgroup_size);

            if offset + total_size <= self.options.max_compute_workgroup_storage_size {
                *workgroup_size = offset + total_size;
            } else {
                storage_class = StorageClass::Private;
            }
        }

        if self.wg_size > 1 && storage_class == StorageClass::WorkGroup {
            let global_name = format!("{name}_wg");
            let array_type = DataType::Array(Rc::new(data_type.clone()), Some(self.wg_size));

            let wg_decl = GlobalVarDecl {
                attrs: vec![],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::WorkGroup,
                    access_mode: None,
                }),
                name: global_name.clone(),
                data_type: array_type.clone(),
                initializer: None,
            };

            let priv_decl = GlobalVarDecl {
                attrs: vec![],
                qualifier: Some(VarQualifier {
                    storage_class: StorageClass::Private,
                    access_mode: None,
                }),
                name: name.clone(),
                data_type: data_type.clone(),
                initializer: None,
            };

            self.global_scope.insert_mutable(
                name.clone(),
                DataType::Ref(MemoryViewType::new(
                    data_type.clone(),
                    StorageClass::Private,
                )),
            );

            self.copy_in_out.push(CopyInOut {
                name,
                global_name,
                data_type,
                wrapper_type: array_type,
                storage_class: StorageClass::WorkGroup,
            });

            vec![wg_decl, priv_decl]
        } else {
            let mem_view = MemoryViewType::new(data_type.clone(), storage_class);
            let ref_type = DataType::Ref(mem_view);

            self.global_scope.insert_mutable(name.clone(), ref_type);

            let initializer = if storage_class == StorageClass::Private && self.rng.gen_bool(0.75) {
                Some(self.gen_const_expr(&data_type))
            } else {
                None
            };

            vec![GlobalVarDecl {
                attrs: vec![],
                qualifier: Some(VarQualifier {
                    storage_class,
                    access_mode: None,
                }),
                name,
                data_type,
                initializer,
            }]
        }
    }

    #[tracing::instrument(skip(self))]
    fn gen_entrypoint_function(
        &mut self,
        in_buf_type: DataType,
        out_buf_type: DataType,
        out_buf_global_type: DataType,
        hash_out_global_type: DataType,
    ) -> FnDecl {
        let prev_is_entrypoint = std::mem::replace(&mut self.fn_state.is_entrypoint, true);

        let mut inputs = vec![];
        if self.wg_size > 1 {
            inputs.push(FnInput {
                attrs: vec![FnParamReturnAttr::Builtin(
                    BuiltinValue::LocalInvocationIndex,
                )],
                name: "local_invocation_index".to_owned(),
                data_type: DataType::Scalar(ScalarType::U32),
            });
        }

        let stmt_count = self.rng.gen_range(5..10);
        let (_, block) = self.with_scope(self.global_scope.clone(), |this| {
            let mut prelude = vec![];
            let idx =
                VarExpr::new("local_invocation_index").into_node(DataType::Scalar(ScalarType::U32));

            if this.wg_size > 1 {
                prelude.extend(divergence::generate_prelude(
                    idx.clone(),
                    this.options.enable_f16,
                    &this.copy_in_out,
                ));
            }

            let (scope, mut block) = this.gen_stmt_block(stmt_count);

            if let Some(Statement::Return(_)) = block.last() {
                block.pop();
            }

            prelude.extend(block);
            block = prelude;

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

                let out_rhs = this.gen_expr(&out_buf_type);

                let out_lhs = if this.wg_size > 1 {
                    let out_buf_global_ref = DataType::Ref(MemoryViewType {
                        inner: Rc::new(out_buf_global_type.clone()),
                        storage_class: StorageClass::Storage,
                        access_mode: AccessMode::ReadWrite,
                    });
                    let wrapper_node = LhsExprNode::name("s_output".to_owned(), out_buf_global_ref);
                    wrapper_node.array_index(idx.clone()).into()
                } else {
                    AssignmentLhs::name("s_output", out_buf_type.clone())
                };

                this.current_block
                    .push(AssignmentStatement::new(out_lhs, AssignmentOp::Simple, out_rhs).into());

                let hash_val = this.gen_scope_hash_expr(&this.scope.clone());

                let hash_lhs = if this.wg_size > 1 {
                    let hash_out_global_ref = DataType::Ref(MemoryViewType {
                        inner: Rc::new(hash_out_global_type.clone()),
                        storage_class: StorageClass::Storage,
                        access_mode: AccessMode::ReadWrite,
                    });
                    let wrapper_node =
                        LhsExprNode::name("hash_output".to_owned(), hash_out_global_ref);
                    wrapper_node.array_index(idx.clone()).into()
                } else {
                    AssignmentLhs::name("hash_output", ScalarType::U32)
                };

                this.current_block.push(
                    AssignmentStatement::new(hash_lhs, AssignmentOp::Simple, hash_val).into(),
                );

                if this.wg_size > 1 {
                    this.current_block.extend(divergence::generate_postlude(
                        idx.clone(),
                        &this.copy_in_out,
                    ));
                }
            });

            std::mem::replace(&mut this.current_block, prev_block)
        });

        self.fn_state.is_entrypoint = prev_is_entrypoint;

        FnDecl {
            attrs: vec![
                FnAttr::Stage(ShaderStage::Compute),
                FnAttr::WorkgroupSize(vec![ExprNode {
                    data_type: DataType::Scalar(ScalarType::U32),
                    expr: Expr::Lit(Lit::U32(self.wg_size)),
                }]),
            ],
            name: "main".to_owned(),
            inputs,
            output: None,
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
