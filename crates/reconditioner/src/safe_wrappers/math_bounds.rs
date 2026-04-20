use ast::{
    DataType, FnCallExpr, FnDecl, FnInput, FnOutput, Lit, ReturnStatement, ScalarType,
    TypeConsExpr, VarExpr,
};

pub fn math_bounds(
    name: String,
    func_name: &str,
    data_type: &DataType,
    min: Option<f32>,
    max: Option<f32>,
) -> FnDecl {
    let x = VarExpr::new("x").into_node(data_type.clone());

    let safe_x = if let Some(min_val) = min {
        if let Some(max_val) = max {
            let (min_lit, max_lit) = match data_type.as_scalar() {
                Some(ScalarType::F16) => (
                    Lit::F16(half::f16::from_f32(min_val)),
                    Lit::F16(half::f16::from_f32(max_val)),
                ),
                _ => (Lit::F32(min_val), Lit::F32(max_val)),
            };
            let min_expr = TypeConsExpr::new(data_type.clone(), vec![min_lit.into()]).into();
            let max_expr = TypeConsExpr::new(data_type.clone(), vec![max_lit.into()]).into();
            FnCallExpr::new("clamp", vec![x, min_expr, max_expr]).into_node(data_type.clone())
        } else {
            let min_lit = match data_type.as_scalar() {
                Some(ScalarType::F16) => Lit::F16(half::f16::from_f32(min_val)),
                _ => Lit::F32(min_val),
            };
            let min_expr = TypeConsExpr::new(data_type.clone(), vec![min_lit.into()]).into();
            FnCallExpr::new("max", vec![x, min_expr]).into_node(data_type.clone())
        }
    } else if let Some(max_val) = max {
        let max_lit = match data_type.as_scalar() {
            Some(ScalarType::F16) => Lit::F16(half::f16::from_f32(max_val)),
            _ => Lit::F32(max_val),
        };
        let max_expr = TypeConsExpr::new(data_type.clone(), vec![max_lit.into()]).into();
        FnCallExpr::new("min", vec![x, max_expr]).into_node(data_type.clone())
    } else {
        x
    };

    let raw_call = FnCallExpr::new(func_name, vec![safe_x]).into_node(data_type.clone());

    FnDecl {
        attrs: vec![],
        name,
        inputs: vec![FnInput::new("x", data_type.clone())],
        output: Some(FnOutput::new(data_type.clone())),
        body: vec![ReturnStatement::new(raw_call).into()],
    }
}
