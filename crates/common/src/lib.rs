#[derive(Debug)]
pub enum ScalarType {
    I32,
    U32,
    F32,
    F16,
}

#[derive(Debug)]
pub enum VectorSize {
    N2,
    N3,
    N4,
}

#[derive(Debug)]
pub struct StructMember {
    pub name: String,
    pub type_desc: Type,
    pub size: Option<u32>,
    pub alignment: Option<u32>,
}

#[derive(Debug)]
pub enum Type {
    Scalar {
        scalar_type: ScalarType,
    },
    Vector {
        size: VectorSize,
        scalar_type: ScalarType,
    },
    Array {
        size: Option<u32>,
        element_type: Box<Type>,
    },
    Struct {
        members: Vec<StructMember>,
    },
}

fn aligned(size: u32, alignment: u32) -> u32 {
    size.div_ceil(alignment) * alignment
}

impl Type {
    pub fn buffer_size(&self) -> u32 {
        aligned(self.size(), self.alignment())
    }

    pub fn size(&self) -> u32 {
        match self {
            Type::Scalar { scalar_type } => match scalar_type {
                ScalarType::F16 => 2,
                _ => 4,
            },
            Type::Vector { size, scalar_type } => {
                let scalar_size = match scalar_type {
                    ScalarType::F16 => 2,
                    _ => 4,
                };
                match size {
                    VectorSize::N2 => scalar_size * 2,
                    VectorSize::N3 => scalar_size * 3,
                    VectorSize::N4 => scalar_size * 4,
                }
            }
            Type::Array { size, element_type } => match size {
                Some(size) => size * aligned(element_type.size(), element_type.alignment()),
                None => aligned(element_type.size(), element_type.alignment()),
            },
            Type::Struct { members } => {
                let mut size = 0;
                let mut alignment = 0;

                for member in members {
                    let member_alignment = member
                        .alignment
                        .unwrap_or_else(|| member.type_desc.alignment());
                    let member_size = member.size.unwrap_or_else(|| member.type_desc.size());

                    alignment = u32::max(alignment, member_alignment);
                    size = aligned(size, member_alignment) + member_size;
                }

                aligned(size, alignment)
            }
        }
    }

    pub fn alignment(&self) -> u32 {
        match self {
            Type::Scalar { scalar_type } => match scalar_type {
                ScalarType::F16 => 2,
                _ => 4,
            },
            Type::Vector { size, scalar_type } => {
                let scalar_align = match scalar_type {
                    ScalarType::F16 => 2,
                    _ => 4,
                };
                match size {
                    VectorSize::N2 => scalar_align * 2,
                    VectorSize::N3 => scalar_align * 4,
                    VectorSize::N4 => scalar_align * 4,
                }
            }
            Type::Array { element_type, .. } => element_type.alignment(),
            Type::Struct { members } => members
                .iter()
                .map(|it| it.alignment.unwrap_or_else(|| it.type_desc.alignment()))
                .max()
                .expect("struct must have at least one member"),
        }
    }

    pub fn ranges(&self, buffer_size: Option<u64>) -> Vec<(usize, usize)> {
        let mut ranges = vec![];

        fn collect_ranges(
            acc: &mut Vec<(usize, usize)>,
            mut offset: u32,
            type_desc: &Type,
            buffer_size: Option<u64>,
        ) {
            match type_desc {
                Type::Scalar { .. } => acc.push((offset as _, type_desc.size() as _)),
                Type::Vector { .. } => acc.push((offset as _, type_desc.size() as _)),
                Type::Array { size, element_type } => {
                    let element_size = element_type.size();
                    let alignment = element_type.alignment();

                    let count = if let Some(size) = size {
                        *size
                    } else if let Some(buffer_size) = buffer_size {
                        let stride = aligned(element_size, alignment);
                        let remaining = buffer_size.saturating_sub(offset as u64);
                        (remaining / stride as u64) as u32
                    } else {
                        1
                    };

                    for _ in 0..count {
                        collect_ranges(acc, offset, element_type, buffer_size);
                        offset = aligned(offset + element_size, alignment);
                    }
                }
                Type::Struct { members } => {
                    for member in members {
                        let alignment = member
                            .alignment
                            .unwrap_or_else(|| member.type_desc.alignment());
                        offset = aligned(offset, alignment);
                        collect_ranges(acc, offset, &member.type_desc, buffer_size);
                        let size = member.size.unwrap_or_else(|| member.type_desc.size());
                        offset += size;
                    }
                }
            }
        }

        collect_ranges(&mut ranges, 0, self, buffer_size);

        ranges
    }
}

impl TryFrom<&ast::ScalarType> for ScalarType {
    type Error = &'static str;

    fn try_from(value: &ast::ScalarType) -> Result<Self, Self::Error> {
        match value {
            ast::ScalarType::Bool => Err("bool is not allowed"),
            ast::ScalarType::I32 => Ok(ScalarType::I32),
            ast::ScalarType::U32 => Ok(ScalarType::U32),
            ast::ScalarType::F32 => Ok(ScalarType::F32),
            ast::ScalarType::F16 => Ok(ScalarType::F16),
        }
    }
}

impl TryFrom<&ast::DataType> for Type {
    type Error = &'static str;

    fn try_from(value: &ast::DataType) -> Result<Self, Self::Error> {
        match value {
            ast::DataType::Scalar(scalar) => Ok(Type::Scalar {
                scalar_type: scalar.try_into()?,
            }),
            ast::DataType::Vector(n, scalar) => Ok(Type::Vector {
                size: match n {
                    2 => VectorSize::N2,
                    3 => VectorSize::N3,
                    4 => VectorSize::N4,
                    _ => return Err("invalid vector size"),
                },
                scalar_type: scalar.try_into()?,
            }),
            ast::DataType::Matrix(c, r, scalar) => Ok(Type::Array {
                size: Some(*c as u32),
                element_type: Box::new(Type::Vector {
                    size: match r {
                        2 => VectorSize::N2,
                        3 => VectorSize::N3,
                        4 => VectorSize::N4,
                        _ => return Err("invalid vector size"),
                    },
                    scalar_type: scalar.try_into()?,
                }),
            }),
            ast::DataType::Array(inner, size) => Ok(Type::Array {
                size: *size,
                element_type: Box::new(inner.as_ref().try_into()?),
            }),
            ast::DataType::Struct(decl) => {
                let mut members = vec![];

                for member in &decl.members {
                    let type_desc = Type::try_from(&member.data_type)?;
                    let mut size = None;
                    let mut alignment = None;

                    for attr in &member.attrs {
                        match attr {
                            ast::StructMemberAttr::Align(n) => alignment = Some(*n),
                            ast::StructMemberAttr::Size(n) => size = Some(*n),
                            ast::StructMemberAttr::Builtin(_) => {}
                            ast::StructMemberAttr::Interpolate(_, _) => {}
                            ast::StructMemberAttr::Location(_) => {}
                        }
                    }

                    members.push(StructMember {
                        name: member.name.clone(),
                        type_desc,
                        size,
                        alignment,
                    });
                }

                Ok(Type::Struct { members })
            }
            ast::DataType::Atomic(scalar) => Ok(Type::Scalar {
                scalar_type: scalar.try_into()?,
            }),
            ast::DataType::AtomicCompareExchangeResult(_) => {
                Err("atomic compare exchange result is not storable")
            }
            ast::DataType::FrexpResult(_) => Err("frexp result is not storable"),
            ast::DataType::ModfResult(_) => Err("modf result is not storable"),
            ast::DataType::Ptr(_) => Err("pointers are not storable"),
            ast::DataType::Ref(_) => Err("references are not storable"),
            ast::DataType::Texture(_) => Err("textures are not storable"),
            ast::DataType::Sampler(_) => Err("samplers are not storable"),
        }
    }
}
