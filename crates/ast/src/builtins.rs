use crate::types::{DataType, ScalarType};
use crate::{TextureDimension, TextureType};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, strum::AsRefStr, strum::EnumIter, strum::EnumString,
)]
#[strum(serialize_all = "camelCase")]
pub enum BuiltinFn {
    Abs,
    Acos,
    Acosh,
    All,
    Any,
    ArrayLength,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Atan2,
    AtomicAdd,
    AtomicAnd,
    AtomicCompareExchangeWeak,
    AtomicExchange,
    AtomicLoad,
    AtomicMax,
    AtomicMin,
    AtomicOr,
    AtomicStore,
    AtomicSub,
    AtomicXor,
    Ceil,
    Clamp,
    Cos,
    Cosh,
    CountLeadingZeros,
    CountOneBits,
    CountTrailingZeros,
    Cross,
    Degrees,
    Determinant,
    Distance,
    Dot,
    #[strum(serialize = "dot4I8Packed")]
    Dot4I8Packed,
    #[strum(serialize = "dot4U8Packed")]
    Dot4U8Packed,
    Dpdx,
    DpdxCoarse,
    DpdxFine,
    Dpdy,
    DpdyCoarse,
    DpdyFine,
    Exp,
    Exp2,
    ExtractBits,
    FaceForward,
    FirstLeadingBit,
    FirstTrailingBit,
    Floor,
    Fma,
    Fract,
    Frexp,
    Fwidth,
    FwidthCoarse,
    FwidthFine,
    InsertBits,
    InverseSqrt,
    Ldexp,
    Length,
    Log,
    Log2,
    Max,
    Min,
    Mix,
    Modf,
    Normalize,
    #[strum(serialize = "pack2x16float")]
    Pack2x16float,
    #[strum(serialize = "pack2x16snorm")]
    Pack2x16snorm,
    #[strum(serialize = "pack2x16unorm")]
    Pack2x16unorm,
    #[strum(serialize = "pack4x8snorm")]
    Pack4x8snorm,
    #[strum(serialize = "pack4x8unorm")]
    Pack4x8unorm,
    #[strum(serialize = "pack4xI8")]
    Pack4xI8,
    #[strum(serialize = "pack4xI8Clamp")]
    Pack4xI8Clamp,
    #[strum(serialize = "pack4xU8")]
    Pack4xU8,
    #[strum(serialize = "pack4xU8Clamp")]
    Pack4xU8Clamp,
    Pow,
    QuantizeToF16,
    Radians,
    Reflect,
    Refract,
    ReverseBits,
    Round,
    Saturate,
    Select,
    ShiftLeft,
    ShiftRight,
    Sign,
    Sin,
    Sinh,
    Smoothstep,
    Sqrt,
    Step,
    Tan,
    Tanh,
    Transpose,
    Trunc,
    #[strum(serialize = "unpack2x16float")]
    Unpack2x16float,
    #[strum(serialize = "unpack2x16snorm")]
    Unpack2x16snorm,
    #[strum(serialize = "unpack2x16unorm")]
    Unpack2x16unorm,
    #[strum(serialize = "unpack4x8snorm")]
    Unpack4x8snorm,
    #[strum(serialize = "unpack4x8unorm")]
    Unpack4x8unorm,
    #[strum(serialize = "unpack4xI8")]
    Unpack4xI8,
    #[strum(serialize = "unpack4xU8")]
    Unpack4xU8,

    // Texture
    TextureDimensions,
    TextureGather,
    TextureGatherCompare,
    TextureLoad,
    TextureNumLayers,
    TextureNumLevels,
    TextureNumSamples,
    TextureSample,
    TextureSampleBias,
    TextureSampleCompare,
    TextureSampleCompareLevel,
    TextureSampleGrad,
    TextureSampleLevel,
    TextureSampleBaseClampToEdge,
    TextureStore,

    // Subgroup
    SubgroupAdd,
    SubgroupAnd,
    SubgroupExclusiveAdd,
    SubgroupInclusiveAdd,
    SubgroupAll,
    SubgroupAny,
    SubgroupBallot,
    SubgroupBroadcast,
    SubgroupBroadcastFirst,
    SubgroupElect,
    SubgroupMax,
    SubgroupMin,
    SubgroupMul,
    SubgroupExclusiveMul,
    SubgroupInclusiveMul,
    SubgroupOr,
    SubgroupShuffle,
    SubgroupShuffleDown,
    SubgroupShuffleUp,
    SubgroupShuffleXor,
    SubgroupXor,

    // Quad
    QuadBroadcast,
    QuadSwapDiagonal,
    QuadSwapX,
    QuadSwapY,

    // Synchronization
    StorageBarrier,
    TextureBarrier,
    WorkgroupBarrier,
    WorkgroupUniformLoad,
}

impl BuiltinFn {
    /// Determines the return type for a builtin function, given argument types.
    ///
    /// Note that this only does the bare minimum work for overload resolution and does not do any
    /// actual validation/type checking. In most cases it only looks at the first argument and
    /// doesn't even validate if the correct number of arguments have been supplied.
    pub fn return_type<'a>(
        &self,
        mut params: impl Iterator<Item = &'a DataType>,
    ) -> Option<DataType> {
        use BuiltinFn::*;
        use ScalarType::*;

        let mut first_param = || params.next().map(DataType::dereference).cloned();

        let ret = match self {
            Abs => first_param()?,
            Acos => first_param()?,
            Acosh => first_param()?,
            Asin => first_param()?,
            Asinh => first_param()?,
            Atan => first_param()?,
            Atanh => first_param()?,
            Atan2 => first_param()?,
            All => Bool.into(),
            AtomicAdd | AtomicAnd | AtomicExchange | AtomicLoad | AtomicMax | AtomicMin
            | AtomicOr | AtomicSub | AtomicXor => {
                let ty = first_param()?;
                if let DataType::Ptr(view) = ty {
                    if let DataType::Atomic(t) = view.inner.as_ref() {
                        DataType::Scalar(*t)
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            AtomicCompareExchangeWeak => {
                let ty = first_param()?;
                if let DataType::Ptr(view) = ty {
                    if let DataType::Atomic(t) = view.inner.as_ref() {
                        DataType::AtomicCompareExchangeResult(*t)
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            AtomicStore => return None,
            Any => Bool.into(),
            ArrayLength => U32.into(),
            Ceil => first_param()?,
            Clamp => first_param()?,
            Cos => first_param()?,
            Cosh => first_param()?,
            CountLeadingZeros => first_param()?,
            CountOneBits => first_param()?,
            CountTrailingZeros => first_param()?,
            Cross => first_param()?,
            Degrees => first_param()?,
            Determinant => {
                if let DataType::Matrix(_, _, s) = first_param()? {
                    DataType::Scalar(s)
                } else {
                    return None;
                }
            }
            Distance => F32.into(),
            Dot => first_param()?.as_scalar()?.into(),
            Dot4I8Packed => I32.into(),
            Dot4U8Packed => U32.into(),
            Dpdx | DpdxCoarse | DpdxFine | Dpdy | DpdyCoarse | DpdyFine => first_param()?,
            ExtractBits => first_param()?,
            Exp => first_param()?,
            Exp2 => first_param()?,
            FaceForward => first_param()?,
            FirstLeadingBit => first_param()?,
            FirstTrailingBit => first_param()?,
            Floor => first_param()?,
            Fma => first_param()?,
            Fract => first_param()?,
            Frexp => DataType::FrexpResult(Box::new(first_param()?)),
            Fwidth | FwidthCoarse | FwidthFine => first_param()?,
            InsertBits => first_param()?,
            InverseSqrt => first_param()?,
            Ldexp => first_param()?,
            Length => F32.into(),
            Log => first_param()?,
            Log2 => first_param()?,
            Max => first_param()?,
            Min => first_param()?,
            Mix => first_param()?,
            Modf => DataType::ModfResult(Box::new(first_param()?)),
            Normalize => first_param()?,
            Pow => first_param()?,
            Pack2x16float | Pack2x16snorm | Pack2x16unorm | Pack4x8snorm | Pack4x8unorm
            | Pack4xI8 | Pack4xI8Clamp | Pack4xU8 | Pack4xU8Clamp => U32.into(),
            QuadBroadcast | QuadSwapX | QuadSwapY | QuadSwapDiagonal => first_param()?,
            QuantizeToF16 => first_param()?,
            Radians => first_param()?,
            Reflect => first_param()?,
            Refract => first_param()?,
            ReverseBits => first_param()?,
            Round => first_param()?,
            Saturate => first_param()?,
            Select => first_param()?,
            ShiftLeft => first_param()?,
            ShiftRight => first_param()?,
            Sign => first_param()?,
            Sin => first_param()?,
            Sinh => first_param()?,
            Smoothstep => first_param()?,
            Sqrt => first_param()?,
            Step => first_param()?,
            Tan => first_param()?,
            Tanh => first_param()?,
            SubgroupBallot => DataType::Vector(4, U32),
            SubgroupBroadcast
            | SubgroupBroadcastFirst
            | SubgroupShuffle
            | SubgroupShuffleXor
            | SubgroupShuffleUp
            | SubgroupShuffleDown => first_param()?,
            SubgroupAdd | SubgroupExclusiveAdd | SubgroupInclusiveAdd | SubgroupMul
            | SubgroupExclusiveMul | SubgroupInclusiveMul | SubgroupMin | SubgroupMax
            | SubgroupAnd | SubgroupOr | SubgroupXor => first_param()?,
            SubgroupAll | SubgroupAny | SubgroupElect => Bool.into(),
            Transpose => {
                if let DataType::Matrix(c, r, s) = first_param()? {
                    DataType::Matrix(r, c, s)
                } else {
                    return None;
                }
            }
            Trunc => first_param()?,
            Unpack2x16float | Unpack2x16snorm | Unpack2x16unorm => DataType::Vector(2, F32),
            Unpack4x8snorm | Unpack4x8unorm => DataType::Vector(4, F32),
            Unpack4xI8 => DataType::Vector(4, I32),
            Unpack4xU8 => DataType::Vector(4, U32),
            TextureDimensions => {
                let ty = first_param()?;
                if let DataType::Texture(t) = ty {
                    let dim = match t {
                        TextureType::Sampled { dim, .. } => dim,
                        TextureType::Multisampled { dim, .. } => dim,
                        TextureType::Storage { dim, .. } => dim,
                        TextureType::Depth { dim, .. } => dim,
                        TextureType::External => TextureDimension::D2,
                    };

                    match dim {
                        TextureDimension::D1 => U32.into(),
                        TextureDimension::D2
                        | TextureDimension::D2Array
                        | TextureDimension::Cube
                        | TextureDimension::CubeArray => DataType::Vector(2, U32),
                        TextureDimension::D3 => DataType::Vector(3, U32),
                    }
                } else {
                    return None;
                }
            }
            TextureGather => {
                let _ = first_param()?;
                let ty = params.next().map(DataType::dereference).cloned()?;
                if let DataType::Texture(t) = ty {
                    match t {
                        TextureType::Sampled { derived_type, .. } => {
                            DataType::Vector(4, derived_type)
                        }
                        TextureType::Multisampled { derived_type, .. } => {
                            DataType::Vector(4, derived_type)
                        }
                        _ => todo!(),
                    }
                } else {
                    DataType::Vector(4, F32)
                }
            }
            TextureGatherCompare => DataType::Vector(4, F32),
            TextureLoad => {
                // Returns vec4<T> or f32 for depth
                let ty = first_param()?;
                if let DataType::Texture(t) = ty {
                    match t {
                        TextureType::Depth { .. } => F32.into(),
                        TextureType::Sampled { derived_type, .. } => {
                            DataType::Vector(4, derived_type)
                        }
                        TextureType::Multisampled { derived_type, .. } => {
                            DataType::Vector(4, derived_type)
                        }
                        TextureType::Storage { format, .. } => {
                            DataType::Vector(4, format.scalar_type())
                        }
                        TextureType::External => DataType::Vector(4, F32),
                    }
                } else {
                    return None;
                }
            }
            TextureNumLayers | TextureNumLevels | TextureNumSamples => U32.into(),
            TextureSample
            | TextureSampleBias
            | TextureSampleCompare
            | TextureSampleCompareLevel
            | TextureSampleGrad
            | TextureSampleLevel
            | TextureSampleBaseClampToEdge => {
                let ty = first_param()?;
                if let DataType::Texture(t) = ty {
                    match t {
                        TextureType::Depth { .. } => F32.into(),
                        _ => DataType::Vector(4, F32),
                    }
                } else {
                    return None;
                }
            }
            StorageBarrier | TextureBarrier | WorkgroupBarrier | TextureStore => return None,
            WorkgroupUniformLoad => {
                let ty = first_param()?;
                if let DataType::Ptr(view) = ty {
                    if let DataType::Atomic(t) = view.inner.as_ref() {
                        DataType::Scalar(*t)
                    } else {
                        view.inner.as_ref().clone()
                    }
                } else {
                    return None;
                }
            }
        };

        Some(ret)
    }
}

#[derive(
    Debug,
    PartialEq,
    Eq,
    Clone,
    Copy,
    Hash,
    strum::AsRefStr,
    strum::Display,
    strum::EnumIter,
    strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
pub enum BuiltinValue {
    VertexIndex,
    InstanceIndex,
    Position,
    FrontFacing,
    FragDepth,
    LocalInvocationId,
    LocalInvocationIndex,
    GlobalInvocationId,
    WorkgroupId,
    NumWorkgroups,
    SampleIndex,
    SampleMask,
    ClipDistances,
    PrimitiveIndex,
    GlobalInvocationIndex,
    WorkgroupIndex,
    SubgroupInvocationId,
    SubgroupSize,
    SubgroupId,
    NumSubgroups,
}
