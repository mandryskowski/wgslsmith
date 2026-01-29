use std::collections::HashMap;
use ast::{Expr, ExprNode, Postfix};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Divergence {
    Uniform,
    Divergent,
}

pub struct DivergenceState {
    divergence_env: HashMap<String, Divergence>,
    divergence_stack: Vec<Divergence>,
}

impl DivergenceState {
    pub fn new() -> Self {
        DivergenceState {
            divergence_env: HashMap::new(),
            divergence_stack: vec![],
        }
    }

    pub fn flow_is_divergent(&self) -> bool {
        self.divergence_stack.contains(&Divergence::Divergent)
    }

    pub fn get_var_divergence(&self, name: &str) -> Divergence {
        if let Some(d) = self.divergence_env.get(name) {
            *d
        } else {
            Divergence::Uniform
        }
    }

    pub fn update_var_divergence(&mut self, name: String, div: Divergence) {
        self.divergence_env.insert(name, div);
    }
}

impl super::Generator<'_> {
    pub fn with_divergence_limit<T>(
        &mut self,
        limit: Divergence,
        f: impl FnOnce(&mut Self) -> T
    ) -> T {
        let old_limit = self.fn_state.divergence_limit;

        // We can only tighten constraints, never loosen them beyond the global flow
        // (e.g., if we are already forcing Uniform, we can't switch to Divergent)
        let new_limit = if old_limit == Divergence::Uniform {
            Divergence::Uniform
        } else {
            limit
        };

        self.fn_state.divergence_limit = new_limit;
        let result = f(self);
        self.fn_state.divergence_limit = old_limit;
        result
    }

    fn infer_expr_divergence(&self, expr: &ExprNode) -> Divergence {
        let reduce_divergences = | divergences: Vec<Divergence> | {
          divergences.into_iter().fold(Divergence::Uniform, |acc, x| {
              if acc == Divergence::Uniform {
                  x
              } else {
                  Divergence::Divergent
              }
          })
        };

        let reduce_exprs = | exprs: Vec<Box<ExprNode>> | -> Divergence {
            reduce_divergences(exprs.iter().map(| x | self.infer_expr_divergence(x)).collect())
        };

        Divergence::Uniform
        // match expr.expr {
        //     Expr::Lit(_) => Divergence::Uniform,
        //     Expr::TypeCons(expr) => reduce_exprs(expr.args),
        //     Expr::Var(expr) => self.divergence_state.get_var_divergence(expr.ident.as_str()),
        //     Expr::Postfix(expr) => {
        //         match expr.postfix {
        //             Postfix::Index(expr) => {}
        //             Postfix::Member(ident) => {}
        //         }
        //     }
        //     Expr::UnOp(expr) => self.infer_expr_divergence(expr.inner.as_ref()),
        //     Expr::BinOp(expr) => reduce_exprs(vec![expr.left, expr.right]),
        //     Expr::FnCall(_) => todo!()
        // }
    }

    fn update_symbol_divergence(&mut self, ident: &String, rhs: &ExprNode) {
        let rhs_div = self.infer_expr_divergence(&rhs);
        let flow_div = if self.divergence_state.flow_is_divergent() { Divergence::Divergent } else { Divergence::Uniform };

        // A variable becomes divergent if assigned a divergent value OR assigned under divergent flow
        let new_div = if rhs_div == Divergence::Divergent || flow_div == Divergence::Divergent {
            Divergence::Divergent
        } else {
            Divergence::Uniform
        };

        self.divergence_state.update_var_divergence(ident.clone(), new_div);
    }
}