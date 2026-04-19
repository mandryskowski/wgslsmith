use ast::{
    BinOp, BinOpExpr, DataType, FnCallExpr, FnDecl, FnInput, FnOutput, ReturnStatement, VarExpr,
};

pub fn smoothstep(name: String, data_type: &DataType) -> FnDecl {
    let edge0 = VarExpr::new("edge0").into_node(data_type.clone());
    let edge1 = VarExpr::new("edge1").into_node(data_type.clone());
    let x = VarExpr::new("x").into_node(data_type.clone());

    let condition = super::componentwise_or(edge0.clone(), edge1.clone(), |a, b| {
        BinOpExpr::new(BinOp::Equal, a, b).into()
    });

    let safe_result = edge0.clone();

    let raw_call =
        FnCallExpr::new("smoothstep", vec![edge0, edge1, x]).into_node(data_type.clone());

    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![
            FnInput::new("edge0", data_type.clone()),
            FnInput::new("edge1", data_type.clone()),
            FnInput::new("x", data_type.clone()),
        ],
        output: Some(FnOutput::new(data_type.clone())),
        body: vec![ReturnStatement::new(
            FnCallExpr::new("select", vec![raw_call, safe_result, condition])
                .into_node(data_type.clone()),
        )
        .into()],
    }
}
