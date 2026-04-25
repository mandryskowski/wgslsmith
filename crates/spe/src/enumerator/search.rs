use crate::enumerator::context::Context;
use crate::enumerator::types::{Hole, HoleType};
use rand::seq::SliceRandom;
use rand::SeedableRng;

pub struct Enumerator {
    pub holes: Vec<Hole>,
    pub results: Vec<Vec<usize>>,
    pub scope_parents: Vec<usize>,
    pub limit: Option<usize>,
    pub step_count: usize,
    pub rng: rand::rngs::StdRng,
}

impl Enumerator {
    pub fn new(ctx: &Context, limit: Option<usize>) -> Self {
        Self {
            holes: ctx.holes.clone(),
            results: vec![],
            scope_parents: ctx.scope_parents.clone(),
            limit,
            step_count: 0,
            rng: rand::rngs::StdRng::seed_from_u64(42),
        }
    }

    pub fn enumerate(&mut self, current: &mut Vec<usize>) {
        if self.holes.is_empty() {
            self.results.push(current.clone());
            return;
        }

        struct Frame {
            ids: std::vec::IntoIter<usize>,
        }

        let mut stack = Vec::new();

        let mut get_ids = |enumerator: &mut Self, current: &[usize]| -> Frame {
            let max_id = current
                .iter()
                .max()
                .copied()
                .map(|m| m as i32)
                .unwrap_or(-1);
            let next_available = (max_id + 1) as usize;

            let mut ids: Vec<usize> = (0..=next_available).collect();
            ids.shuffle(&mut enumerator.rng);

            Frame {
                ids: ids.into_iter(),
            }
        };

        stack.push(get_ids(self, current));
        self.step_count += 1;

        while let Some(frame) = stack.last_mut() {
            if self.step_count > 100_000 {
                break;
            }
            if let Some(lim) = self.limit {
                if self.results.len() >= lim {
                    break;
                }
            }

            let hole_idx = current.len();
            let mut found = false;

            while let Some(id) = frame.ids.next() {
                if self.is_valid_assignment(hole_idx, id, current) {
                    current.push(id);
                    found = true;
                    break;
                }
            }

            if found {
                self.step_count += 1;
                if current.len() == self.holes.len() {
                    self.results.push(current.clone());
                    current.pop();
                } else {
                    stack.push(get_ids(self, current));
                }
            } else {
                stack.pop();
                if !current.is_empty() {
                    current.pop();
                }
            }
        }
    }

    pub fn is_ancestor(&self, possible_ancestor: usize, mut node: usize) -> bool {
        if possible_ancestor == node {
            return true;
        }

        while node != 0 {
            let parent = self.scope_parents[node];
            if parent == possible_ancestor {
                return true;
            }
            if parent == node {
                break;
            }
            node = parent;
        }
        possible_ancestor == 0
    }

    pub fn is_valid_assignment(&self, hole_idx: usize, id: usize, current: &[usize]) -> bool {
        let hole = &self.holes[hole_idx];

        let mut is_reused = false;
        let mut visible_decl_scope = None;

        for (prev_idx, &prev_id) in current.iter().enumerate() {
            if prev_id == id {
                is_reused = true;
                let prev_hole = &self.holes[prev_idx];

                if prev_hole.data_type.dereference() != hole.data_type.dereference() {
                    return false;
                }

                if let HoleType::Decl(prev_decl) = &prev_hole.hole_type {
                    if self.is_ancestor(prev_hole.scope_id, hole.scope_id) {
                        if let HoleType::Decl(_) = hole.hole_type {
                            if prev_hole.scope_id == hole.scope_id {
                                return false;
                            }
                        }
                        visible_decl_scope = Some(prev_hole.scope_id);
                    }

                    if let HoleType::Usage(usage) = &hole.hole_type {
                        if !usage.is_satisfied_by(prev_decl) {
                            return false;
                        }
                    }
                }
            }
        }

        match &hole.hole_type {
            HoleType::Decl(_) => {
                if is_reused && visible_decl_scope.is_none() {
                    return false;
                }
            }
            HoleType::Usage(_) => {
                if visible_decl_scope.is_none() {
                    return false;
                }
            }
        }

        true
    }
}
