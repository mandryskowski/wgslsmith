// hash.rs
use ast::types::{DataType, ScalarType};
use ast::{
    BinOp, BinOpExpr, Expr, ExprNode, FnCallExpr, Lit, Postfix, PostfixExpr, TypeConsExpr, VarExpr,
};

use crate::gen::scope::Scope;
use crate::gen::Generator;

impl<'a> Generator<'a> {
    pub fn gen_scope_hash_expr(&mut self, scope: &Scope) -> ExprNode {
        let mut symbols = scope.all_symbols();

        symbols.sort_by(|a, b| a.0.cmp(&b.0));

        let mut hash_exprs = Vec::new();

        for (name, ty) in symbols {
            let value_type = ty.dereference();

            if self.is_hashable_type(value_type) {
                let var_node = VarExpr::new(name).into_node(ty.clone());

                // let value_expr = if ty.is() || ty.is_ptr() {
                //     UnOpExpr::new(UnOp::Deref, var_node).into()
                // } else {
                //     var_node
                // };
                let value_expr = var_node;

                let u32_expr = self.convert_to_u32_hash(value_expr, value_type);
                hash_exprs.push(u32_expr);
            }
        }

        if hash_exprs.is_empty() {
            return Lit::U32(0).into();
        }

        let mut acc = hash_exprs.remove(0);

        for expr in hash_exprs {
            // acc * 33
            let mul: ExprNode =
                BinOpExpr::new(BinOp::Times, acc, ExprNode::from(Lit::U32(33))).into();

            // ^ val
            acc = BinOpExpr::new(BinOp::BitXOr, mul, expr).into();
        }

        acc
    }

    fn is_hashable_type(&self, ty: &DataType) -> bool {
        match ty {
            DataType::Scalar(_) => true,
            DataType::Vector(_, _) => true,
            // Complex types skipped for simplicity for now
            _ => false,
        }
    }

    fn convert_to_u32_hash(&mut self, expr: ExprNode, ty: &DataType) -> ExprNode {
        match ty {
            DataType::Scalar(ScalarType::U32) => expr,

            DataType::Scalar(ScalarType::I32) | DataType::Scalar(ScalarType::F32) => {
                self.gen_bitcast_expr(expr, DataType::Scalar(ScalarType::U32))
            }
            DataType::Scalar(ScalarType::F16) => {
                // f16 -> f32 -> u32 (bitcast)
                let f32_expr =
                    TypeConsExpr::new(DataType::Scalar(ScalarType::F32), vec![expr]).into();
                self.gen_bitcast_expr(f32_expr, DataType::Scalar(ScalarType::U32))
            }

            // Bitcast is often invalid for bools due to size ambiguity, so we use constructor.
            DataType::Scalar(ScalarType::Bool) => {
                TypeConsExpr::new(DataType::Scalar(ScalarType::U32), vec![expr]).into()
            }

            DataType::Vector(n, sub_ty) => {
                let vec_u32_ty = DataType::Vector(*n, ScalarType::U32);

                let cast_expr = if matches!(sub_ty, ScalarType::U32) {
                    expr
                } else if matches!(sub_ty, ScalarType::Bool) {
                    // vecN<bool> -> vecN<u32> constructor
                    TypeConsExpr::new(vec_u32_ty.clone(), vec![expr]).into()
                } else if matches!(sub_ty, ScalarType::F16) {
                    // vecN<f16> -> vecN<f32> -> bitcast<vecN<u32>>
                    let vec_f32_ty = DataType::Vector(*n, ScalarType::F32);
                    let f32_expr = TypeConsExpr::new(vec_f32_ty, vec![expr]).into();
                    self.gen_bitcast_expr(f32_expr, vec_u32_ty.clone())
                } else {
                    // vecN<i32|f32> -> bitcast<vecN<u32>>
                    self.gen_bitcast_expr(expr, vec_u32_ty.clone())
                };

                let components = ["x", "y", "z", "w"];
                let mut acc =
                    PostfixExpr::new(cast_expr.clone(), Postfix::member(components[0])).into();

                for component in components.iter().take(*n as usize).skip(1) {
                    let member: ExprNode =
                        PostfixExpr::new(cast_expr.clone(), Postfix::member(*component)).into();

                    acc = BinOpExpr::new(BinOp::BitXOr, acc, member).into();
                }

                acc
            }

            _ => unreachable!(),
        }
    }

    fn gen_bitcast_expr(&self, expr: ExprNode, target_ty: DataType) -> ExprNode {
        let node = FnCallExpr {
            ident: "bitcast".to_owned(),
            template_args: vec![target_ty.clone()],
            args: vec![expr],
        };

        ExprNode {
            data_type: target_ty,
            expr: Expr::FnCall(node),
        }
    }
}
