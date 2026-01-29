use std::collections::HashMap;
use rand::Rng;
use ast::{Expr, ExprNode, Postfix};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Divergence {
    Uniform,
    Divergent,
}

impl Default for Divergence {
    fn default() -> Self {
        Self::Uniform
    }
}

impl Divergence {
    pub fn join(self, other: Divergence) -> Self {
        match (self, other) {
            (Self::Uniform, Self::Uniform) => Self::Uniform,
            _ => Self::Divergent,
        }
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

    pub(crate) fn sample_divergence(&mut self) -> Option<Divergence> {
        if self.scope.flow_divergence == Divergence::Divergent {
            None
        } else {
            Some(if self.rng.gen_bool(0.2) {
                Divergence::Divergent
            } else {
                Divergence::Uniform
            })
        }
    }

    pub fn infer_expr_divergence(&self, expr: &ExprNode) -> Divergence {
        // Helper to combine divergence of children: Uniform + Uniform = Uniform, else Divergent
        let reduce = |divs: Vec<Divergence>| {
            if divs.into_iter().any(|d| d == Divergence::Divergent) {
                Divergence::Divergent
            } else {
                Divergence::Uniform
            }
        };

        match &expr.expr {
            Expr::Lit(_) => Divergence::Uniform,

            Expr::Var(v) => self.scope.get_divergence(&v.ident),

            Expr::TypeCons(c) => {
                reduce(c.args.iter().map(|arg| self.infer_expr_divergence(arg)).collect())
            },

            Expr::UnOp(u) => self.infer_expr_divergence(&u.inner),

            Expr::BinOp(b) => {
                reduce(vec![
                    self.infer_expr_divergence(&b.left),
                    self.infer_expr_divergence(&b.right)
                ])
            },

            Expr::FnCall(f) => {
                // Assume that result is divergent if any arg is divergent.
                reduce(f.args.iter().map(|arg| self.infer_expr_divergence(arg)).collect())
            },

            Expr::Postfix(p) => {
                // If the base (struct/array) is divergent, the member is divergent.
                // If the base is uniform, the member is uniform.
                // Note: Array indexing with a divergent index makes the result divergent,
                // but for Postfix member access, it depends on the base.
                let base_div = self.infer_expr_divergence(&p.inner);
                match &p.postfix {
                    Postfix::Index(idx) => {
                        let idx_div = self.infer_expr_divergence(idx);
                        if base_div == Divergence::Divergent || idx_div == Divergence::Divergent {
                            Divergence::Divergent
                        } else {
                            Divergence::Uniform
                        }
                    },
                    Postfix::Member(_) => base_div,
                }
            }
        }
    }

    pub fn update_symbol_divergence(&mut self, ident: &String, rhs: &ExprNode) {
        let data_div = self.infer_expr_divergence(rhs);
        let flow_div = if self.scope.flow_divergence == Divergence::Divergent {
            Divergence::Divergent
        } else {
            Divergence::Uniform
        };

        let final_div = if data_div == Divergence::Divergent || flow_div == Divergence::Divergent {
            Divergence::Divergent
        } else {
            Divergence::Uniform
        };

        self.scope.set_divergence(ident.clone(), Some(final_div));

        tracing::debug!("Updated divergence for {}: {:?} (Data: {:?}, Flow: {:?})",
                ident, final_div, data_div, flow_div);
    }
}