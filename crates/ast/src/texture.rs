use std::fmt::{self, Display};

use crate::AccessMode;
use crate::ScalarType;
use derive_more::Display;

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    strum::EnumString,
    strum::EnumIter,
    strum::AsRefStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum TextureDimension {
    #[display("1d")]
    #[strum(serialize = "1d")]
    D1,
    #[display("2d")]
    #[strum(serialize = "2d")]
    D2,
    #[display("2d_array")]
    #[strum(serialize = "2d_array")]
    D2Array,
    #[display("3d")]
    #[strum(serialize = "3d")]
    D3,
    #[display("cube")]
    #[strum(serialize = "cube")]
    Cube,
    #[display("cube_array")]
    #[strum(serialize = "cube_array")]
    CubeArray,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    strum::EnumString,
    strum::EnumIter,
    strum::AsRefStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum SamplerType {
    #[display("sampler")]
    #[strum(serialize = "sampler")]
    Sampler,
    #[display("sampler_comparison")]
    #[strum(serialize = "sampler_comparison")]
    Comparison,
}

#[derive(
    Clone,
    Copy,
    Debug,
    Display,
    Hash,
    PartialEq,
    Eq,
    strum::EnumString,
    strum::EnumIter,
    strum::AsRefStr,
)]
#[strum(serialize_all = "lowercase")]
pub enum TexelFormat {
    #[display("rgba8unorm")]
    Rgba8Unorm,
    #[display("rgba8snorm")]
    Rgba8Snorm,
    #[display("rgba8uint")]
    Rgba8Uint,
    #[display("rgba8sint")]
    Rgba8Sint,
    #[display("rgba16unorm")]
    Rgba16Unorm,
    #[display("rgba16snorm")]
    Rgba16Snorm,
    #[display("rgba16uint")]
    Rgba16Uint,
    #[display("rgba16sint")]
    Rgba16Sint,
    #[display("rgba16float")]
    Rgba16Float,
    #[display("rg8unorm")]
    Rg8Unorm,
    #[display("rg8snorm")]
    Rg8Snorm,
    #[display("rg8uint")]
    Rg8Uint,
    #[display("rg8sint")]
    Rg8Sint,
    #[display("rg16unorm")]
    Rg16Unorm,
    #[display("rg16snorm")]
    Rg16Snorm,
    #[display("rg16uint")]
    Rg16Uint,
    #[display("rg16sint")]
    Rg16Sint,
    #[display("rg16float")]
    Rg16Float,
    #[display("r32uint")]
    R32Uint,
    #[display("r32sint")]
    R32Sint,
    #[display("r32float")]
    R32Float,
    #[display("rg32uint")]
    Rg32Uint,
    #[display("rg32sint")]
    Rg32Sint,
    #[display("rg32float")]
    Rg32Float,
    #[display("rgba32uint")]
    Rgba32Uint,
    #[display("rgba32sint")]
    Rgba32Sint,
    #[display("rgba32float")]
    Rgba32Float,
    #[display("bgra8unorm")]
    Bgra8Unorm,
    #[display("bgra8unorm_srgb")]
    Bgra8UnormSrgb,
    #[display("r8unorm")]
    R8Unorm,
    #[display("r8snorm")]
    R8Snorm,
    #[display("r8uint")]
    R8Uint,
    #[display("r8sint")]
    R8Sint,
    #[display("r16unorm")]
    R16Unorm,
    #[display("r16snorm")]
    R16Snorm,
    #[display("r16uint")]
    R16Uint,
    #[display("r16sint")]
    R16Sint,
    #[display("r16float")]
    R16Float,
    #[display("rgb10a2unorm")]
    Rgb10A2Unorm,
    #[display("rgb10a2uint")]
    Rgb10A2Uint,
    #[display("rg11b10ufloat")]
    Rg11B10Ufloat,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum TextureType {
    Sampled {
        dim: TextureDimension,
        derived_type: ScalarType,
    },
    Multisampled {
        dim: TextureDimension,
        derived_type: ScalarType,
    },
    Storage {
        dim: TextureDimension,
        format: TexelFormat,
        access: AccessMode,
    },
    Depth {
        dim: TextureDimension,
        clone_of_multisampled: bool,
    },
    External,
}

impl Display for TextureType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureType::Sampled { dim, derived_type } => {
                write!(f, "texture_{}<{}>", dim, derived_type)
            }
            TextureType::Multisampled { dim, derived_type } => match dim {
                TextureDimension::D2 => write!(f, "texture_multisampled_2d<{}>", derived_type),
                _ => unreachable!("Only 2D multisampled textures are supported"),
            },
            TextureType::Storage {
                dim,
                format,
                access,
            } => write!(f, "texture_storage_{}<{}, {}>", dim, format, access),
            TextureType::Depth {
                dim,
                clone_of_multisampled,
            } => {
                if *clone_of_multisampled {
                    match dim {
                        TextureDimension::D2 => write!(f, "texture_depth_multisampled_2d"),
                        _ => unreachable!("Only 2D multisampled depth textures are supported"),
                    }
                } else {
                    write!(f, "texture_depth_{}", dim)
                }
            }
            TextureType::External => write!(f, "texture_external"),
        }
    }
}
