use bincode::{Decode, Encode};

pub struct ResourceData<'a> {
    pub name: &'a str,
    pub group: u32,
    pub binding: u32,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, Hash)]
pub enum EnableExtension {
    F16,
    Subgroups,
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct PipelineDescription {
    pub resources: Vec<PipelineResource>,
    pub enables: Vec<EnableExtension>,
    pub entry_point: String,
}

#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, Hash)]
pub enum TextureDimension {
    D1,
    D2,
    D2Array,
    D3,
    Cube,
    CubeArray,
}

#[derive(Clone, Copy, Debug, Decode, Encode, PartialEq, Eq, Hash)]
pub enum TextureFormat {
    Rgba8Unorm,
    Rgba8Snorm,
    Rgba8Uint,
    Rgba8Sint,
    Rgba16Unorm,
    Rgba16Snorm,
    Rgba16Uint,
    Rgba16Sint,
    Rgba16Float,
    Rg8Unorm,
    Rg8Snorm,
    Rg8Uint,
    Rg8Sint,
    Rg16Unorm,
    Rg16Snorm,
    Rg16Uint,
    Rg16Sint,
    Rg16Float,
    R32Uint,
    R32Sint,
    R32Float,
    Rg32Uint,
    Rg32Sint,
    Rg32Float,
    Rgba32Uint,
    Rgba32Sint,
    Rgba32Float,
    Bgra8Unorm,
    R8Unorm,
    R8Snorm,
    R8Uint,
    R8Sint,
    R16Unorm,
    R16Snorm,
    R16Uint,
    R16Sint,
    R16Float,
    Rgb10A2Unorm,
    Rgb10A2Uint,
    Rg11B10Ufloat,
}

impl TextureFormat {
    pub fn is_uint(&self) -> bool {
        matches!(
            self,
            TextureFormat::Rgba8Uint
                | TextureFormat::Rgba16Uint
                | TextureFormat::Rg8Uint
                | TextureFormat::Rg16Uint
                | TextureFormat::R32Uint
                | TextureFormat::Rg32Uint
                | TextureFormat::Rgba32Uint
                | TextureFormat::R8Uint
                | TextureFormat::R16Uint
                | TextureFormat::Rgb10A2Uint
        )
    }

    pub fn is_sint(&self) -> bool {
        matches!(
            self,
            TextureFormat::Rgba8Sint
                | TextureFormat::Rgba16Sint
                | TextureFormat::Rg8Sint
                | TextureFormat::Rg16Sint
                | TextureFormat::R32Sint
                | TextureFormat::Rg32Sint
                | TextureFormat::Rgba32Sint
                | TextureFormat::R8Sint
                | TextureFormat::R16Sint
        )
    }

    pub fn is_float(&self) -> bool {
        matches!(
            self,
            TextureFormat::Rgba8Unorm
                | TextureFormat::Rgba8Snorm
                | TextureFormat::Rgba16Unorm
                | TextureFormat::Rgba16Snorm
                | TextureFormat::Rgba16Float
                | TextureFormat::Rg8Unorm
                | TextureFormat::Rg8Snorm
                | TextureFormat::Rg16Unorm
                | TextureFormat::Rg16Snorm
                | TextureFormat::Rg16Float
                | TextureFormat::R32Float
                | TextureFormat::Rg32Float
                | TextureFormat::Rgba32Float
                | TextureFormat::Bgra8Unorm
                | TextureFormat::R8Unorm
                | TextureFormat::R8Snorm
                | TextureFormat::R16Unorm
                | TextureFormat::R16Snorm
                | TextureFormat::R16Float
                | TextureFormat::Rgb10A2Unorm
                | TextureFormat::Rg11B10Ufloat
        )
    }
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq, Hash)]
pub enum SamplerKind {
    Sampler,
    Comparison,
}

#[derive(Clone, Debug, Decode, Encode, PartialEq, Eq)]
pub enum ResourceKind {
    StorageBuffer,
    UniformBuffer,
    Texture {
        dim: TextureDimension,
        format: Option<TextureFormat>,
    },
    Sampler {
        kind: SamplerKind,
    },
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct PipelineResource {
    pub name: String,
    pub kind: ResourceKind,
    pub group: u32,
    pub binding: u32,
    pub init: Option<Vec<u8>>,
    pub size: u32,
}
