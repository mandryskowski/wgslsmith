use crate::enumerator::context::Context;
use crate::enumerator::types::{Hole, HoleType};
use rand::seq::SliceRandom;
use rand::SeedableRng;

pub struct Enumerator {
    pub holes: Vec<Hole>,
    pub results: Vec<Vec<usize>>,
    pub scope_parents: Vec<usize>,
    pub limit: Option<usize>,
    pub rng: rand::rngs::StdRng,
    pub is_ancestor_matrix: Vec<bool>,
    pub num_scopes: usize,
}

impl Enumerator {
    pub fn new(ctx: &Context, limit: Option<usize>) -> Self {
        let num_scopes = ctx.scope_parents.len();
        let mut is_ancestor_matrix = vec![false; num_scopes * num_scopes];

        for desc in 0..num_scopes {
            let mut curr = desc;
            loop {
                is_ancestor_matrix[curr * num_scopes + desc] = true;
                if curr == 0 {
                    break;
                }
                let p = ctx.scope_parents[curr];
                if p == curr {
                    break;
                }
                curr = p;
            }
        }

        Self {
            holes: ctx.holes.clone(),
            results: vec![],
            scope_parents: ctx.scope_parents.clone(),
            limit,
            rng: rand::rngs::StdRng::seed_from_u64(42),
            is_ancestor_matrix,
            num_scopes,
        }
    }

    #[inline(always)]
    pub fn is_ancestor_fast(&self, anc: usize, desc: usize) -> bool {
        self.is_ancestor_matrix[anc * self.num_scopes + desc]
    }

    pub fn enumerate(&mut self, current: &mut Vec<usize>) {
        if self.holes.is_empty() {
            self.results.push(current.clone());
            return;
        }
        if self.holes.len() > 10_000 {
            self.results.push(current.clone());
            println!(
                "Skeleton has too many holes: {}. Skipping enumeration to avoid stack exhaustion.",
                self.holes.len()
            );
            return;
        }
        self.solve_recursive(current);
    }

    fn solve_recursive(&mut self, current: &mut Vec<usize>) {
        if let Some(lim) = self.limit {
            if self.results.len() >= lim {
                return;
            }
        }

        let hole_idx = current.len();
        if hole_idx == self.holes.len() {
            self.results.push(current.clone());
            return;
        }

        let hole = &self.holes[hole_idx];

        let max_id = current
            .iter()
            .max()
            .copied()
            .map(|m| m as i32)
            .unwrap_or(-1);
        let next_id = (max_id + 1) as usize;

        let mut valid_ids = Vec::new();

        if let HoleType::Decl(_) = hole.hole_type {
            valid_ids.push(next_id);
        }

        let mut id_visible_decl = std::collections::HashMap::new();
        for (prev_idx, &prev_id) in current.iter().enumerate() {
            let prev_hole = &self.holes[prev_idx];
            if let HoleType::Decl(_) = &prev_hole.hole_type {
                if self.is_ancestor_fast(prev_hole.scope_id, hole.scope_id) {
                    id_visible_decl.insert(prev_id, prev_hole);
                }
            }
        }

        for (&id, &prev_hole) in &id_visible_decl {
            if prev_hole.data_type.dereference() != hole.data_type.dereference() {
                continue;
            }

            match &hole.hole_type {
                HoleType::Decl(_) => {
                    if prev_hole.scope_id != hole.scope_id {
                        valid_ids.push(id);
                    }
                }
                HoleType::Usage(usage) => {
                    if let HoleType::Decl(prev_decl) = &prev_hole.hole_type {
                        if usage.is_satisfied_by(prev_decl) {
                            valid_ids.push(id);
                        }
                    }
                }
            }
        }

        valid_ids.shuffle(&mut self.rng);

        for id in valid_ids {
            current.push(id);
            self.solve_recursive(current);
            current.pop();

            if let Some(lim) = self.limit {
                if self.results.len() >= lim {
                    return;
                }
            }
        }
    }
}
