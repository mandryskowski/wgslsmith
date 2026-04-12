use ast::types::{DataType, MemoryViewType, ScalarType};
use ast::{
    AssignmentLhs, AssignmentOp, AssignmentStatement, BinOp, BinOpExpr, ExprNode, FnCallExpr,
    GlobalVarDecl, LhsExprNode, Lit, Postfix, PostfixExpr, Statement, StorageClass, VarExpr,
    VarQualifier,
};

use crate::gen::scope::Scope;

#[derive(Clone)]
pub struct CopyInOut {
    pub name: String,
    pub global_name: String,
    pub data_type: DataType,
    pub wrapper_type: DataType,
    pub storage_class: StorageClass,
}

pub fn generate_divergent_globals(scope: &mut Scope, enable_f16: bool) -> Vec<GlobalVarDecl> {
    let mut global_vars = vec![];

    for (name, ty) in [
        ("divergent_u32", DataType::Scalar(ScalarType::U32)),
        ("divergent_i32", DataType::Scalar(ScalarType::I32)),
        ("divergent_f32", DataType::Scalar(ScalarType::F32)),
        ("divergent_bool", DataType::Scalar(ScalarType::Bool)),
    ] {
        global_vars.push(GlobalVarDecl {
            attrs: vec![],
            qualifier: Some(VarQualifier {
                storage_class: StorageClass::Private,
                access_mode: None,
            }),
            name: name.to_owned(),
            data_type: ty.clone(),
            initializer: None,
        });
        scope.insert_readonly(name.to_owned(), ty.clone());
    }

    if enable_f16 {
        global_vars.push(GlobalVarDecl {
            attrs: vec![],
            qualifier: Some(VarQualifier {
                storage_class: StorageClass::Private,
                access_mode: None,
            }),
            name: "divergent_f16".to_owned(),
            data_type: DataType::Scalar(ScalarType::F16),
            initializer: None,
        });
        scope.insert_readonly(
            "divergent_f16".to_owned(),
            DataType::Scalar(ScalarType::F16),
        );
    }

    global_vars
}

pub fn generate_prelude(idx: ExprNode, enable_f16: bool, copies: &[CopyInOut]) -> Vec<Statement> {
    let mut prelude = vec![
        AssignmentStatement::new(
            AssignmentLhs::name(
                "divergent_u32",
                DataType::Ref(MemoryViewType::new(
                    DataType::Scalar(ScalarType::U32),
                    StorageClass::Private,
                )),
            ),
            AssignmentOp::Simple,
            idx.clone(),
        )
        .into(),
        AssignmentStatement::new(
            AssignmentLhs::name(
                "divergent_i32",
                DataType::Ref(MemoryViewType::new(
                    DataType::Scalar(ScalarType::I32),
                    StorageClass::Private,
                )),
            ),
            AssignmentOp::Simple,
            FnCallExpr::new("i32", vec![idx.clone()]).into_node(DataType::Scalar(ScalarType::I32)),
        )
        .into(),
        AssignmentStatement::new(
            AssignmentLhs::name(
                "divergent_f32",
                DataType::Ref(MemoryViewType::new(
                    DataType::Scalar(ScalarType::F32),
                    StorageClass::Private,
                )),
            ),
            AssignmentOp::Simple,
            FnCallExpr::new("f32", vec![idx.clone()]).into_node(DataType::Scalar(ScalarType::F32)),
        )
        .into(),
        AssignmentStatement::new(
            AssignmentLhs::name(
                "divergent_bool",
                DataType::Ref(MemoryViewType::new(
                    DataType::Scalar(ScalarType::Bool),
                    StorageClass::Private,
                )),
            ),
            AssignmentOp::Simple,
            BinOpExpr::new(
                BinOp::Equal,
                BinOpExpr::new(BinOp::BitAnd, idx.clone(), Lit::U32(1)),
                Lit::U32(1),
            ),
        )
        .into(),
    ];

    if enable_f16 {
        prelude.push(
            AssignmentStatement::new(
                AssignmentLhs::name(
                    "divergent_f16",
                    DataType::Ref(MemoryViewType::new(
                        DataType::Scalar(ScalarType::F16),
                        StorageClass::Private,
                    )),
                ),
                AssignmentOp::Simple,
                FnCallExpr::new("f16", vec![idx.clone()])
                    .into_node(DataType::Scalar(ScalarType::F16)),
            )
            .into(),
        );
    }

    for copy in copies {
        let wrapper_ref = DataType::Ref(MemoryViewType::new(
            copy.wrapper_type.clone(),
            copy.storage_class,
        ));
        let wrapper_node = VarExpr::new(&copy.global_name).into_node(wrapper_ref);
        let element_node: ExprNode =
            PostfixExpr::new(wrapper_node, Postfix::index(idx.clone())).into();

        prelude.push(
            AssignmentStatement::new(
                AssignmentLhs::name(
                    &copy.name,
                    DataType::Ref(MemoryViewType::new(
                        copy.data_type.clone(),
                        StorageClass::Private,
                    )),
                ),
                AssignmentOp::Simple,
                element_node,
            )
            .into(),
        );
    }

    prelude
}

pub fn generate_postlude(idx: ExprNode, copies: &[CopyInOut]) -> Vec<Statement> {
    let mut postlude = vec![];
    for copy in copies {
        let wrapper_ref = DataType::Ref(MemoryViewType::new(
            copy.wrapper_type.clone(),
            copy.storage_class,
        ));
        let wrapper_node = LhsExprNode::name(copy.global_name.clone(), wrapper_ref);
        let element_node = wrapper_node.array_index(idx.clone());

        postlude.push(
            AssignmentStatement::new(
                element_node.into(),
                AssignmentOp::Simple,
                VarExpr::new(&copy.name).into_node(copy.data_type.clone()),
            )
            .into(),
        );
    }
    postlude
}
