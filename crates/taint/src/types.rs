use std::collections::HashSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TaintSet {
    pub origins: HashSet<u32>,
}

impl TaintSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn single(origin: u32) -> Self {
        let mut origins = HashSet::new();
        origins.insert(origin);
        Self { origins }
    }

    pub fn union(&self, other: &Self) -> Self {
        let mut origins = self.origins.clone();
        origins.extend(other.origins.iter().copied());
        Self { origins }
    }

    pub fn is_mixed(&self) -> bool {
        self.origins.len() > 1
    }

    pub fn has_foreign(&self, current: u32) -> bool {
        self.origins.iter().any(|&o| o != current)
    }
}
