use std::rc::Rc;

use ast::types::{DataType, ScalarType};
use ast::{StructDecl, StructMember, StructMemberAttr};
use rand::Rng;

use super::cx::SelectionFilter;

const FIELD_NAMES: &[&str] = &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StructKind {
    Default,
    HostShareable,
    UniformBuffer,
}

pub(crate) fn round_up(alignment: u32, offset: u32) -> u32 {
    if alignment == 0 {
        return offset;
    }
    offset.div_ceil(alignment) * alignment
}

pub(crate) fn align_of(ty: &DataType) -> u32 {
    match ty {
        DataType::Scalar(s) => match s {
            ScalarType::F16 => 2,
            _ => 4,
        },
        DataType::Vector(n, s) => {
            let s_align = align_of(&DataType::Scalar(*s));
            match n {
                2 => 2 * s_align,
                3 => 4 * s_align,
                4 => 4 * s_align,
                _ => unreachable!(),
            }
        }
        DataType::Matrix(_, r, s) => align_of(&DataType::Vector(*r, *s)),
        DataType::Array(t, _) => align_of(t.as_ref()),
        DataType::Struct(decl) => {
            let mut align = 1;
            for member in &decl.members {
                let mut m_align = align_of(&member.data_type);
                for attr in &member.attrs {
                    if let StructMemberAttr::Align(a) = attr {
                        m_align = *a;
                    }
                }
                align = align.max(m_align);
            }
            align
        }
        _ => 1,
    }
}

pub(crate) fn size_of(ty: &DataType) -> u32 {
    match ty {
        DataType::Scalar(s) => match s {
            ScalarType::F16 => 2,
            _ => 4,
        },
        DataType::Vector(n, s) => *n as u32 * size_of(&DataType::Scalar(*s)),
        DataType::Matrix(c, r, s) => {
            let vec_ty = DataType::Vector(*r, *s);
            let stride = round_up(align_of(&vec_ty), size_of(&vec_ty));
            *c as u32 * stride
        }
        DataType::Array(t, n) => {
            let n = n.unwrap_or(1);
            let stride = round_up(align_of(t.as_ref()), size_of(t.as_ref()));
            n * stride
        }
        DataType::Struct(decl) => {
            let mut offset = 0;
            let mut struct_align = 1;
            for member in &decl.members {
                let mut m_align = align_of(&member.data_type);
                let mut m_size = size_of(&member.data_type);
                for attr in &member.attrs {
                    match attr {
                        StructMemberAttr::Align(a) => m_align = *a,
                        StructMemberAttr::Size(s) => m_size = *s,
                        _ => {}
                    }
                }
                struct_align = struct_align.max(m_align);
                offset = round_up(m_align, offset);
                offset += m_size;
            }
            round_up(struct_align, offset)
        }
        _ => 0,
    }
}

impl super::Generator<'_> {
    pub fn gen_struct(&mut self, name: String) -> Rc<StructDecl> {
        self.gen_struct_with(name, StructKind::Default)
    }

    pub fn gen_struct_with(&mut self, name: String, kind: StructKind) -> Rc<StructDecl> {
        let member_count = self
            .rng
            .gen_range(self.options.min_struct_members..=self.options.max_struct_members);

        let filter = match kind {
            StructKind::Default => SelectionFilter::Any,
            StructKind::HostShareable => SelectionFilter::HostShareable,
            StructKind::UniformBuffer => SelectionFilter::Uniform,
        };

        let mut members = (0..member_count)
            .map(|i| {
                StructMember::new(
                    vec![],
                    FIELD_NAMES[i as usize].to_owned(),
                    self.cx.types.select_with_filter(self.rng, filter),
                )
            })
            .collect::<Vec<_>>();

        for member in &mut members {
            let align_of_t = align_of(&member.data_type);
            let size_of_t = size_of(&member.data_type);

            let mut req_align = align_of_t;
            if kind == StructKind::UniformBuffer
                && matches!(
                    member.data_type,
                    DataType::Struct(_) | DataType::Array(_, _)
                )
            {
                // RequiredAlignOf(S, C) for uniform address space is roundUp(16, AlignOf(S))
                req_align = round_up(16, req_align);
            }

            let mut align_val = req_align;
            if self.rng.gen_bool(0.3) {
                align_val *= 1 << self.rng.gen_range(0..=2);
            }
            if align_val > align_of_t || self.rng.gen_bool(0.2) {
                Rc::get_mut(member)
                    .unwrap()
                    .attrs
                    .push(StructMemberAttr::Align(align_val));
            }

            let mut min_size = size_of_t;
            if kind == StructKind::UniformBuffer {
                if let DataType::Struct(_) = member.data_type {
                    // If a structure member itself has a structure type S, then the number of bytes
                    // between the start of that member and the start of any following member must be
                    // at least roundUp(16, SizeOf(S)).
                    min_size = min_size.max(round_up(16, size_of_t));
                }
            }

            let mut size_val = min_size;
            if self.rng.gen_bool(0.3) {
                size_val += self.rng.gen_range(0..=4) * 4;
            }
            if size_val > size_of_t || self.rng.gen_bool(0.2) {
                Rc::get_mut(member)
                    .unwrap()
                    .attrs
                    .push(StructMemberAttr::Size(size_val));
            }
        }

        if kind == StructKind::Default {
            use rand::prelude::SliceRandom;
            let mut next_location = 0;

            for member in &mut members {
                if !self.rng.gen_bool(0.25) {
                    continue;
                }

                let mut valid_builtins = match &member.data_type {
                    DataType::Scalar(ScalarType::U32) => vec![
                        ast::BuiltinValue::VertexIndex,
                        ast::BuiltinValue::InstanceIndex,
                        ast::BuiltinValue::LocalInvocationIndex,
                        ast::BuiltinValue::SampleIndex,
                        ast::BuiltinValue::SampleMask,
                    ],
                    DataType::Scalar(ScalarType::Bool) => vec![ast::BuiltinValue::FrontFacing],
                    DataType::Scalar(ScalarType::F32) => vec![ast::BuiltinValue::FragDepth],
                    DataType::Vector(4, ScalarType::F32) => vec![ast::BuiltinValue::Position],
                    DataType::Vector(3, ScalarType::U32) => vec![
                        ast::BuiltinValue::LocalInvocationId,
                        ast::BuiltinValue::GlobalInvocationId,
                        ast::BuiltinValue::WorkgroupId,
                        ast::BuiltinValue::NumWorkgroups,
                    ],
                    _ => vec![],
                };

                if self.options.collectives() {
                    if let DataType::Scalar(ScalarType::U32) = &member.data_type {
                        valid_builtins.push(ast::BuiltinValue::SubgroupInvocationId);
                        valid_builtins.push(ast::BuiltinValue::SubgroupSize);
                    }
                }

                if !valid_builtins.is_empty() && self.rng.gen_bool(0.5) {
                    let builtin = valid_builtins.choose(self.rng).unwrap();
                    Rc::get_mut(member)
                        .unwrap()
                        .attrs
                        .push(StructMemberAttr::Builtin(*builtin));

                    if *builtin == ast::BuiltinValue::Position && self.rng.gen_bool(0.5) {
                        Rc::get_mut(member)
                            .unwrap()
                            .attrs
                            .push(StructMemberAttr::Invariant);
                    }
                } else {
                    let valid_for_location = match &member.data_type {
                        DataType::Scalar(s) | DataType::Vector(_, s) => {
                            matches!(
                                s,
                                ScalarType::F32
                                    | ScalarType::I32
                                    | ScalarType::U32
                                    | ScalarType::F16
                            )
                        }
                        _ => false,
                    };

                    if valid_for_location {
                        let location = next_location;
                        next_location += 1;
                        Rc::get_mut(member)
                            .unwrap()
                            .attrs
                            .push(StructMemberAttr::Location(location));

                        let valid_for_interpolate = match &member.data_type {
                            DataType::Scalar(s) | DataType::Vector(_, s) => {
                                matches!(s, ScalarType::F32 | ScalarType::F16)
                            }
                            _ => false,
                        };

                        if valid_for_interpolate && self.rng.gen_bool(0.5) {
                            let interp_type = [
                                ast::InterpolationType::Perspective,
                                ast::InterpolationType::Linear,
                                ast::InterpolationType::Flat,
                            ]
                            .choose(self.rng)
                            .unwrap();

                            let interp_sampling = if *interp_type == ast::InterpolationType::Flat {
                                if self.rng.gen_bool(0.5) {
                                    Some(
                                        *[
                                            ast::InterpolationSampling::First,
                                            ast::InterpolationSampling::Either,
                                        ]
                                        .choose(self.rng)
                                        .unwrap(),
                                    )
                                } else {
                                    None
                                }
                            } else if self.rng.gen_bool(0.5) {
                                Some(
                                    *[
                                        ast::InterpolationSampling::Center,
                                        ast::InterpolationSampling::Centroid,
                                        ast::InterpolationSampling::Sample,
                                    ]
                                    .choose(self.rng)
                                    .unwrap(),
                                )
                            } else {
                                None
                            };

                            Rc::get_mut(member)
                                .unwrap()
                                .attrs
                                .push(StructMemberAttr::Interpolate(*interp_type, interp_sampling));
                        } else if !valid_for_interpolate {
                            let interp_sampling = if self.rng.gen_bool(0.5) {
                                Some(
                                    *[
                                        ast::InterpolationSampling::First,
                                        ast::InterpolationSampling::Either,
                                    ]
                                    .choose(self.rng)
                                    .unwrap(),
                                )
                            } else {
                                None
                            };

                            Rc::get_mut(member)
                                .unwrap()
                                .attrs
                                .push(StructMemberAttr::Interpolate(
                                    ast::InterpolationType::Flat,
                                    interp_sampling,
                                ));
                        }
                    }
                }
            }
        }

        StructDecl::new(name, members)
    }
}
