use ast::{DataType, FnCallExpr, FnDecl, FnInput, FnOutput, ReturnStatement, VarExpr};

// TODO: Remove once tint fixes short-circuiting of select for dx12
pub fn select(name: String, ty: &DataType, cond_ty: &DataType) -> FnDecl {
    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![
            FnInput::new("f", ty.clone()),
            FnInput::new("t", ty.clone()),
            FnInput::new("cond", cond_ty.clone()),
        ],
        output: Some(FnOutput::new(ty.clone())),
        body: vec![ReturnStatement::new(
            FnCallExpr::new(
                "select",
                vec![
                    VarExpr::new("f").into_node(ty.clone()),
                    VarExpr::new("t").into_node(ty.clone()),
                    VarExpr::new("cond").into_node(cond_ty.clone()),
                ],
            )
            .into_node(ty.clone()),
        )
        .into()],
    }
}
