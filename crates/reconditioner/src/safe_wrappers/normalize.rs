use ast::{
    BinOp, BinOpExpr, DataType, FnCallExpr, FnDecl, FnInput, FnOutput, IfStatement, Lit,
    ReturnStatement, ScalarType, VarExpr,
};

pub fn normalize(name: String, data_type: &DataType) -> FnDecl {
    let e = VarExpr::new("e").into_node(data_type.clone());

    let condition = super::componentwise_and(e.clone(), e.clone(), |a, _| {
        let zero = match data_type.as_scalar() {
            Some(ScalarType::F16) => Lit::F16(half::f16::from_f32(0.0)),
            _ => Lit::F32(0.0),
        };
        BinOpExpr::new(BinOp::Equal, a, zero).into()
    });

    let raw_call = FnCallExpr::new("normalize", vec![e.clone()]).into_node(data_type.clone());

    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![FnInput::new("e", data_type.clone())],
        output: Some(FnOutput::new(data_type.clone())),
        body: vec![
            IfStatement::new(condition, vec![ReturnStatement::new(e.clone()).into()]).into(),
            ReturnStatement::new(raw_call).into(),
        ],
    }
}
