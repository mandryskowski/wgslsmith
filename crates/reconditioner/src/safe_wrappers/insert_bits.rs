use ast::{
    BinOp, BinOpExpr, DataType, FnCallExpr, FnDecl, FnInput, FnOutput, Lit, ReturnStatement,
    ScalarType, VarExpr,
};
// TODO: Remove once tint fixes dx12 insertBits logic for the case when offset+count overflows U32_MAX.

pub fn insert_bits(name: String, data_type: &DataType) -> FnDecl {
    let scalar = match data_type {
        DataType::Scalar(scalar) => scalar,
        DataType::Vector(_, scalar) => scalar,
        _ => panic!("can only insertBits into a scalar or vector"),
    };
    match scalar {
        ScalarType::I32 | ScalarType::U32 => {}
        _ => panic!("insertBits reconditioning only supported for 32bit integers"),
    };

    let u32_node_type: DataType = ScalarType::U32.into();
    let lit_32 = Lit::U32(32);

    let offset_var = VarExpr::new("offset").into_node(u32_node_type.clone());
    let count_var = VarExpr::new("count").into_node(u32_node_type.clone());

    let safe_offset =
        FnCallExpr::new("min", vec![offset_var.clone(), lit_32.into()]).into_node(u32_node_type.clone());

    let remaining_space = BinOpExpr::new(BinOp::Minus, lit_32, safe_offset.clone());

    let safe_count = FnCallExpr::new("min", vec![count_var.clone(), remaining_space.into()])
        .into_node(u32_node_type.clone());

    // In WGSL, u32 addition wraps. If the sum is less than an operand, it overflowed.
    let sum = BinOpExpr::new(BinOp::Plus, offset_var.clone(), count_var.clone());

    let is_overflow = BinOpExpr::new(BinOp::Less, sum, offset_var.clone());

    let safe_call = FnCallExpr::new(
        "insertBits",
        vec![
            VarExpr::new("e").into_node(data_type.clone()),
            VarExpr::new("newbits").into_node(data_type.clone()),
            safe_offset,
            safe_count,
        ],
    )
    .into_node(data_type.clone());

    let raw_call = FnCallExpr::new(
        "insertBits",
        vec![
            VarExpr::new("e").into_node(data_type.clone()),
            VarExpr::new("newbits").into_node(data_type.clone()),
            offset_var,
            count_var,
        ],
    )
    .into_node(data_type.clone());

    let select_expr = FnCallExpr::new("select", vec![raw_call, safe_call, is_overflow.into()])
        .into_node(data_type.clone());

    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![
            FnInput::new("e", data_type.clone()),
            FnInput::new("newbits", data_type.clone()),
            FnInput::new("offset", ScalarType::U32),
            FnInput::new("count", ScalarType::U32),
        ],
        output: Some(FnOutput::new(data_type.clone())),
        body: vec![ReturnStatement::new(select_expr).into()],
    }
}
