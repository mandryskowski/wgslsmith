use ast::{
    DataType, FnCallExpr, FnDecl, FnInput, FnOutput, Lit, ReturnStatement, ScalarType,
    TypeConsExpr, VarExpr,
};

pub fn pack2x16float(name: String) -> FnDecl {
    let data_type = DataType::Vector(2, ScalarType::F32);
    let out_type = DataType::Scalar(ScalarType::U32);

    let min_val = TypeConsExpr::new(
        data_type.clone(),
        vec![Lit::F32(-65504.0).into(), Lit::F32(-65504.0).into()],
    )
    .into();

    let max_val = TypeConsExpr::new(
        data_type.clone(),
        vec![Lit::F32(65504.0).into(), Lit::F32(65504.0).into()],
    )
    .into();

    let clamped = FnCallExpr::new(
        "clamp",
        vec![
            VarExpr::new("e").into_node(data_type.clone()),
            min_val,
            max_val,
        ],
    )
    .into_node(data_type.clone());

    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![FnInput::new("e", data_type.clone())],
        output: Some(FnOutput::new(out_type.clone())),
        body: vec![ReturnStatement::new(
            FnCallExpr::new("pack2x16float", vec![clamped]).into_node(out_type),
        )
        .into()],
    }
}
