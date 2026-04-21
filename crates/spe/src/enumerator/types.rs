use ast::types::DataType;
use ast::StorageClass;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeclFlags {
    pub mutable: bool,
    pub is_const: bool,
    pub banned_from_vertex: bool,
    pub banned_from_fragment: bool,
    pub storage_class: Option<StorageClass>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclHole {
    pub mutable: bool,
    pub is_const: bool,
    pub banned_from_vertex: bool,
    pub banned_from_fragment: bool,
    pub storage_class: Option<StorageClass>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageHole {
    pub is_lvalue: bool,
    pub is_address_of: bool,
    pub requires_const: bool,
    pub in_vertex_stage: bool,
    pub in_fragment_stage: bool,
    pub expected_storage_class: Option<StorageClass>,
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
        if self.in_fragment_stage && decl.banned_from_fragment {
            return false;
        }
        if self.is_address_of && self.expected_storage_class != decl.storage_class {
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
