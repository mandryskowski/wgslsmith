use ast::{
    BinOp, BinOpExpr, DataType, ExprNode, FnCallExpr, FnDecl, FnInput, FnOutput, Lit,
    ReturnStatement, ScalarType, TypeConsExpr, VarExpr,
};

// TODO: Remove once tint fixes dx12 extractBits logic when:
// 1. signed
// - e is negative and offset is greater than type width
// - c is 0
// - offset + count overflows U32_MAX
// 2. unsigned
// - offset + count overflows U32_MAX

pub fn extract_bits(name: String, data_type: &DataType) -> FnDecl {
    gen_extract_bits_common(name, data_type, |e, offset, count| {
        let e_is_negative = BinOpExpr::new(
            BinOp::Less,
            e.clone(),
            TypeConsExpr::new(data_type.clone(), vec![Lit::I32(0).into()]),
        );

        let negative_check = if let DataType::Vector(..) = data_type {
            FnCallExpr::new("any", vec![e_is_negative.into()]).into_node(ScalarType::Bool)
        } else {
            e_is_negative.into()
        };

        let count_is_zero = BinOpExpr::new(BinOp::Equal, count.clone(), Lit::U32(0));

        let sum = BinOpExpr::new(BinOp::Plus, offset.clone(), count.clone());
        let is_overflow = BinOpExpr::new(BinOp::Less, sum, offset.clone());

        let scalar = match data_type {
            DataType::Scalar(scalar) => scalar,
            DataType::Vector(_, scalar) => scalar,
            _ => panic!("can only extractBits from a scalar or vector"),
        };
        let type_width = match scalar {
            ScalarType::I32 | ScalarType::U32 => 32,
            _ => panic!("extractBits reconditioning only supported for 32bit"),
        };

        // (count == 0) || (overflow) || (e < 0 && offset >= width)
        BinOpExpr::new(
            BinOp::LogOr,
            count_is_zero,
            BinOpExpr::new(
                BinOp::LogOr,
                is_overflow,
                BinOpExpr::new(
                    BinOp::LogAnd,
                    negative_check,
                    BinOpExpr::new(BinOp::GreaterEqual, offset.clone(), Lit::U32(type_width)),
                ),
            ),
        )
        .into()
    })
}

pub fn extract_bits_unsigned(name: String, data_type: &DataType) -> FnDecl {
    gen_extract_bits_common(name, data_type, |_e, offset, count| {
        // (offset + count < offset)
        let sum = BinOpExpr::new(BinOp::Plus, offset.clone(), count.clone());

        BinOpExpr::new(BinOp::Less, sum, offset.clone()).into()
    })
}

// helper
fn gen_extract_bits_common<F>(name: String, data_type: &DataType, condition_builder: F) -> FnDecl
where
    F: FnOnce(ExprNode, ExprNode, ExprNode) -> ExprNode,
{
    let e_var = VarExpr::new("e").into_node(data_type.clone());
    let offset_var = VarExpr::new("offset").into_node(ScalarType::U32.into());
    let count_var = VarExpr::new("count").into_node(ScalarType::U32.into());

    let scalar_type = match data_type {
        DataType::Scalar(s) => s,
        DataType::Vector(_, s) => s,
        _ => panic!("extractBits only supports scalar or vector types"),
    };

    let zero_lit = match scalar_type {
        ScalarType::I32 => Lit::I32(0),
        ScalarType::U32 => Lit::U32(0),
        _ => panic!("extractBits only supported for I32 or U32"),
    };

    let should_return_zero =
        condition_builder(e_var.clone(), offset_var.clone(), count_var.clone());

    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![
            FnInput::new("e", data_type.clone()),
            FnInput::new("offset", ScalarType::U32),
            FnInput::new("count", ScalarType::U32),
        ],
        output: Some(FnOutput::new(data_type.clone())),
        body: vec![ReturnStatement::new(
            FnCallExpr::new(
                "select",
                vec![
                    FnCallExpr::new("extractBits", vec![e_var, offset_var, count_var])
                        .into_node(data_type.clone()),
                    TypeConsExpr::new(data_type.clone(), vec![zero_lit.into()]).into(),
                    should_return_zero,
                ],
            )
            .into_node(data_type.clone()),
        )
        .into()],
    }
}
