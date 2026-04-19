use ast::types::DataType;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclHole {
    pub mutable: bool,
    pub is_const: bool,
    pub banned_from_vertex: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageHole {
    pub is_lvalue: bool,
    pub requires_const: bool,
    pub in_vertex_stage: bool,
}

impl UsageHole {
    pub fn is_satisfied_by(&self, decl: &DeclHole) -> bool {
        if self.is_lvalue && !decl.mutable {
            return false;
        }
        if self.requires_const && !decl.is_const {
            return false;
        }
        if self.in_vertex_stage && decl.banned_from_vertex {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HoleType {
    Decl(DeclHole),
    Usage(UsageHole),
}

#[derive(Clone, Debug)]
pub struct Hole {
    pub hole_type: HoleType,
    pub data_type: DataType,
    pub scope_id: usize,
    #[allow(dead_code)]
    pub original_name: String,
}
