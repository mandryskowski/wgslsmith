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
    pub recursive_calls: usize,
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
            recursive_calls: 0,
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
            println!(
                "Skeleton has too many holes: {}. Skipping enumeration to avoid stack exhaustion.",
                self.holes.len()
            );
            return;
        }

        self.recursive_calls = 0;
        self.solve_recursive(current, false, 100_000, false);

        if self.recursive_calls >= 100_000 {
            self.results.clear();

            let limit = self.limit.unwrap_or(2000);
            for _ in 0..limit {
                self.recursive_calls = 0;
                if !self.solve_recursive(current, true, 10_000, false)
                    && self.recursive_calls < 10_000
                {
                    break;
                }
            }

            if self.results.len() < limit {
                for _ in 0..limit {
                    if self.results.len() >= limit {
                        break;
                    }
                    self.recursive_calls = 0;
                    if !self.solve_recursive(current, true, 10_000, true)
                        && self.recursive_calls < 10_000
                    {
                        break;
                    }
                }
            }
        }
    }

    fn solve_recursive(
        &mut self,
        current: &mut Vec<usize>,
        break_on_first: bool,
        recursive_limit: usize,
        disallow_shadowing: bool,
    ) -> bool {
        self.recursive_calls += 1;
        if self.recursive_calls >= recursive_limit {
            return false;
        }

        let hole_idx = current.len();
        if hole_idx == self.holes.len() {
            if break_on_first && self.results.contains(current) {
                return false;
            }
            self.results.push(current.clone());
            return true;
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
                    if prev_hole.scope_id != hole.scope_id && !disallow_shadowing {
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
            let found =
                self.solve_recursive(current, break_on_first, recursive_limit, disallow_shadowing);
            current.pop();

            if found && break_on_first {
                return true;
            }

            if self.recursive_calls >= recursive_limit {
                return false;
            }
        }

        false
    }
}
