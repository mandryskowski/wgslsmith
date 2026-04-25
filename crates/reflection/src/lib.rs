use ast::{Module, StorageClass, VarQualifier};
pub use types::{
    EnableExtension, PipelineDescription, PipelineResource, ResourceData, ResourceKind,
    SamplerKind, ShaderStage, TextureDimension, TextureFormat,
};

pub fn reflect(
    module: &Module,
    input_data: &std::collections::HashMap<String, Vec<u8>>,
) -> (PipelineDescription, Vec<Option<common::Type>>) {
    let mut resources = vec![];
    let mut types = vec![];

    for var in &module.vars {
        if let Some(VarQualifier { storage_class, .. }) = &var.qualifier {
            let kind = match storage_class {
                StorageClass::Uniform => ResourceKind::UniformBuffer,
                StorageClass::Storage => ResourceKind::StorageBuffer,
                StorageClass::Handle => match &var.data_type {
                    ast::DataType::Texture(t) => {
                        let (dim, format) = match t {
                            ast::TextureType::Sampled { dim, derived_type } => {
                                let format = match derived_type {
                                    ast::ScalarType::I32 => ast::TexelFormat::Rgba8Sint,
                                    ast::ScalarType::U32 => ast::TexelFormat::Rgba8Uint,
                                    _ => ast::TexelFormat::Rgba8Unorm,
                                };
                                (*dim, Some(format))
                            }
                            ast::TextureType::Multisampled { dim, derived_type } => {
                                let format = match derived_type {
                                    ast::ScalarType::I32 => ast::TexelFormat::Rgba8Sint,
                                    ast::ScalarType::U32 => ast::TexelFormat::Rgba8Uint,
                                    _ => ast::TexelFormat::Rgba8Unorm,
                                };
                                (*dim, Some(format))
                            }
                            ast::TextureType::Storage { dim, format, .. } => (*dim, Some(*format)),
                            ast::TextureType::Depth { dim, .. } => (*dim, None),
                            ast::TextureType::External => (
                                ast::TextureDimension::D2,
                                Some(ast::TexelFormat::Rgba8Unorm),
                            ),
                        };

                        let dim = match dim {
                            ast::TextureDimension::D1 => types::TextureDimension::D1,
                            ast::TextureDimension::D2 => types::TextureDimension::D2,
                            ast::TextureDimension::D2Array => types::TextureDimension::D2Array,
                            ast::TextureDimension::D3 => types::TextureDimension::D3,
                            ast::TextureDimension::Cube => types::TextureDimension::Cube,
                            ast::TextureDimension::CubeArray => types::TextureDimension::CubeArray,
                        };

                        let format = format.map(|format| match format {
                            ast::TexelFormat::Rgba8Unorm => types::TextureFormat::Rgba8Unorm,
                            ast::TexelFormat::Rgba8Snorm => types::TextureFormat::Rgba8Snorm,
                            ast::TexelFormat::Rgba8Uint => types::TextureFormat::Rgba8Uint,
                            ast::TexelFormat::Rgba8Sint => types::TextureFormat::Rgba8Sint,
                            ast::TexelFormat::Rgba16Unorm => types::TextureFormat::Rgba16Unorm,
                            ast::TexelFormat::Rgba16Snorm => types::TextureFormat::Rgba16Snorm,
                            ast::TexelFormat::Rgba16Uint => types::TextureFormat::Rgba16Uint,
                            ast::TexelFormat::Rgba16Sint => types::TextureFormat::Rgba16Sint,
                            ast::TexelFormat::Rgba16Float => types::TextureFormat::Rgba16Float,
                            ast::TexelFormat::Rg8Unorm => types::TextureFormat::Rg8Unorm,
                            ast::TexelFormat::Rg8Snorm => types::TextureFormat::Rg8Snorm,
                            ast::TexelFormat::Rg8Uint => types::TextureFormat::Rg8Uint,
                            ast::TexelFormat::Rg8Sint => types::TextureFormat::Rg8Sint,
                            ast::TexelFormat::Rg16Unorm => types::TextureFormat::Rg16Unorm,
                            ast::TexelFormat::Rg16Snorm => types::TextureFormat::Rg16Snorm,
                            ast::TexelFormat::Rg16Uint => types::TextureFormat::Rg16Uint,
                            ast::TexelFormat::Rg16Sint => types::TextureFormat::Rg16Sint,
                            ast::TexelFormat::Rg16Float => types::TextureFormat::Rg16Float,
                            ast::TexelFormat::R32Uint => types::TextureFormat::R32Uint,
                            ast::TexelFormat::R32Sint => types::TextureFormat::R32Sint,
                            ast::TexelFormat::R32Float => types::TextureFormat::R32Float,
                            ast::TexelFormat::Rg32Uint => types::TextureFormat::Rg32Uint,
                            ast::TexelFormat::Rg32Sint => types::TextureFormat::Rg32Sint,
                            ast::TexelFormat::Rg32Float => types::TextureFormat::Rg32Float,
                            ast::TexelFormat::Rgba32Uint => types::TextureFormat::Rgba32Uint,
                            ast::TexelFormat::Rgba32Sint => types::TextureFormat::Rgba32Sint,
                            ast::TexelFormat::Rgba32Float => types::TextureFormat::Rgba32Float,
                            ast::TexelFormat::Bgra8Unorm => types::TextureFormat::Bgra8Unorm,
                            ast::TexelFormat::Bgra8UnormSrgb => {
                                types::TextureFormat::Bgra8UnormSrgb
                            }
                            ast::TexelFormat::R8Unorm => types::TextureFormat::R8Unorm,
                            ast::TexelFormat::R8Snorm => types::TextureFormat::R8Snorm,
                            ast::TexelFormat::R8Uint => types::TextureFormat::R8Uint,
                            ast::TexelFormat::R8Sint => types::TextureFormat::R8Sint,
                            ast::TexelFormat::R16Unorm => types::TextureFormat::R16Unorm,
                            ast::TexelFormat::R16Snorm => types::TextureFormat::R16Snorm,
                            ast::TexelFormat::R16Uint => types::TextureFormat::R16Uint,
                            ast::TexelFormat::R16Sint => types::TextureFormat::R16Sint,
                            ast::TexelFormat::R16Float => types::TextureFormat::R16Float,
                            ast::TexelFormat::Rgb10A2Unorm => types::TextureFormat::Rgb10A2Unorm,
                            ast::TexelFormat::Rgb10A2Uint => types::TextureFormat::Rgb10A2Uint,
                            ast::TexelFormat::Rg11B10Ufloat => types::TextureFormat::Rg11B10Ufloat,
                        });

                        ResourceKind::Texture { dim, format }
                    }
                    ast::DataType::Sampler(s) => ResourceKind::Sampler {
                        kind: match *s {
                            ast::SamplerType::Sampler => types::SamplerKind::Sampler,
                            ast::SamplerType::Comparison => types::SamplerKind::Comparison,
                        },
                    },
                    _ => continue,
                },
                _ => continue,
            };

            let type_desc = common::Type::try_from(&var.data_type).ok();

            let group = var
                .group_index()
                .expect("resource variable must have group attribute");

            let binding = var
                .binding_index()
                .expect("resource variable must have binding attribute");

            let mut min_size = type_desc.as_ref().map(|t| t.buffer_size()).unwrap_or(0);
            min_size = (min_size + 3) & !3;

            let init = input_data
                .get(&format!("{group}:{binding}"))
                .cloned()
                .map(|mut init| {
                    if min_size > 0 && init.len() < min_size as usize {
                        init.resize(min_size as usize, 0);
                    }
                    init
                });

            let size = init.as_ref().map(|it| it.len() as u32).unwrap_or(min_size);

            resources.push(PipelineResource {
                name: var.name.clone(),
                kind,
                group,
                binding,
                init,
                size,
            });

            types.push(type_desc);
        }
    }

    let enables = module
        .enables
        .iter()
        .map(|e| match e {
            ast::EnableExtension::F16 => types::EnableExtension::F16,
            ast::EnableExtension::Subgroups => types::EnableExtension::Subgroups,
        })
        .collect();

    let mut entry_points = vec![];
    for f in &module.functions {
        for attr in &f.attrs {
            if let ast::FnAttr::Stage(stage) = attr {
                let reflection_stage = match stage {
                    ast::ShaderStage::Compute => types::ShaderStage::Compute,
                    ast::ShaderStage::Vertex => types::ShaderStage::Vertex,
                    ast::ShaderStage::Fragment => types::ShaderStage::Fragment,
                };
                entry_points.push((f.name.clone(), reflection_stage));
            }
        }
    }

    if entry_points.is_empty() {
        entry_points.push(("main".to_owned(), types::ShaderStage::Compute));
    }

    (
        PipelineDescription {
            resources,
            enables,
            entry_points,
        },
        types,
    )
}
