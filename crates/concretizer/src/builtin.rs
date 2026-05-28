use crate::concretizer::{in_float16_range, in_float_range};
use crate::value;
use ast::*;
use value::Value;

#[derive(Debug)]
pub enum Builtin {
    Abs,
    Acos,
    Acosh,
    Asin,
    Atanh,
    Atan2,
    Clamp,
    All,
    Any,
    Determinant,
    Dot,
    Dot4I8Packed,
    Dot4U8Packed,
    Exp,
    Exp2,
    CountLeadingZeros,
    CountTrailingZeros,
    CountOneBits,
    ExtractBits,
    InsertBits,
    ReverseBits,
    FirstLeadingBit,
    FirstTrailingBit,
    Min,
    Max,
    Pack2x16float,
    Pack2x16snorm,
    Pack2x16unorm,
    Pack4x8snorm,
    Pack4x8unorm,
    Pack4xI8,
    Pack4xI8Clamp,
    Pack4xU8,
    Pack4xU8Clamp,
    Select,
    Smoothstep,
    Unpack2x16float,
    Unpack2x16snorm,
    Unpack2x16unorm,
    Unpack4x8snorm,
    Unpack4x8unorm,
    Unpack4xI8,
    Unpack4xU8,
    Atan,
    Cos,
    Cosh,
    Degrees,
    InverseSqrt,
    Log,
    Log2,
    Radians,
    Sin,
    Sinh,
    Sqrt,
    Tan,
    Tanh,
    Fract,
    Ceil,
    Floor,
    Trunc,
    Round,
    Saturate,
    Sign,
    QuantizeToF16,
    Step,
    Pow,
    Ldexp,
    Normalize,
    Transpose,
    Fma,
    Mix,
    Length,
    Distance,
    Cross,
    FaceForward,
    Reflect,
    Refract,
    SubgroupAdd,
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
    QuadBroadcast,
    QuadSwapDiagonal,
    QuadSwapX,
    QuadSwapY,
    WorkgroupUniformLoad,
}

impl Builtin {
    pub fn convert(ident: String) -> Option<Builtin> {
        match ident.as_str() {
            "atan" => Some(Builtin::Atan),
            "cos" => Some(Builtin::Cos),
            "cosh" => Some(Builtin::Cosh),
            "degrees" => Some(Builtin::Degrees),
            "inverseSqrt" => Some(Builtin::InverseSqrt),
            "log" => Some(Builtin::Log),
            "log2" => Some(Builtin::Log2),
            "radians" => Some(Builtin::Radians),
            "sin" => Some(Builtin::Sin),
            "sinh" => Some(Builtin::Sinh),
            "sqrt" => Some(Builtin::Sqrt),
            "tan" => Some(Builtin::Tan),
            "tanh" => Some(Builtin::Tanh),
            "fract" => Some(Builtin::Fract),
            "ceil" => Some(Builtin::Ceil),
            "floor" => Some(Builtin::Floor),
            "trunc" => Some(Builtin::Trunc),
            "round" => Some(Builtin::Round),
            "saturate" => Some(Builtin::Saturate),
            "sign" => Some(Builtin::Sign),
            "quantizeToF16" => Some(Builtin::QuantizeToF16),
            "step" => Some(Builtin::Step),
            "pow" => Some(Builtin::Pow),
            "ldexp" => Some(Builtin::Ldexp),
            "fma" => Some(Builtin::Fma),
            "mix" => Some(Builtin::Mix),
            "length" => Some(Builtin::Length),
            "distance" => Some(Builtin::Distance),
            "cross" => Some(Builtin::Cross),
            "faceForward" => Some(Builtin::FaceForward),
            "reflect" => Some(Builtin::Reflect),
            "refract" => Some(Builtin::Refract),
            "clamp" => Some(Builtin::Clamp),
            "exp" => Some(Builtin::Exp),
            "exp2" => Some(Builtin::Exp2),
            "all" => Some(Builtin::All),
            "any" => Some(Builtin::Any),
            "abs" => Some(Builtin::Abs),
            "acos" => Some(Builtin::Acos),
            "acosh" => Some(Builtin::Acosh),
            "asin" => Some(Builtin::Asin),
            "atanh" => Some(Builtin::Atanh),
            "atan2" => Some(Builtin::Atan2),
            "normalize" => Some(Builtin::Normalize),
            "transpose" => Some(Builtin::Transpose),
            "countLeadingZeros" => Some(Builtin::CountLeadingZeros),
            "countTrailingZeros" => Some(Builtin::CountTrailingZeros),
            "countOneBits" => Some(Builtin::CountOneBits),
            "determinant" => Some(Builtin::Determinant),
            "dot" => Some(Builtin::Dot),
            "dot4I8Packed" => Some(Builtin::Dot4I8Packed),
            "dot4U8Packed" => Some(Builtin::Dot4U8Packed),
            "extractBits" => Some(Builtin::ExtractBits),
            "insertBits" => Some(Builtin::InsertBits),
            "reverseBits" => Some(Builtin::ReverseBits),
            "firstLeadingBit" => Some(Builtin::FirstLeadingBit),
            "firstTrailingBit" => Some(Builtin::FirstTrailingBit),
            "min" => Some(Builtin::Min),
            "max" => Some(Builtin::Max),
            "pack2x16float" => Some(Builtin::Pack2x16float),
            "pack2x16snorm" => Some(Builtin::Pack2x16snorm),
            "pack2x16unorm" => Some(Builtin::Pack2x16unorm),
            "pack4x8snorm" => Some(Builtin::Pack4x8snorm),
            "pack4x8unorm" => Some(Builtin::Pack4x8unorm),
            "pack4xI8" => Some(Builtin::Pack4xI8),
            "pack4xI8Clamp" => Some(Builtin::Pack4xI8Clamp),
            "pack4xU8" => Some(Builtin::Pack4xU8),
            "pack4xU8Clamp" => Some(Builtin::Pack4xU8Clamp),
            "select" => Some(Builtin::Select),
            "smoothstep" => Some(Builtin::Smoothstep),
            "unpack2x16float" => Some(Builtin::Unpack2x16float),
            "unpack2x16snorm" => Some(Builtin::Unpack2x16snorm),
            "unpack2x16unorm" => Some(Builtin::Unpack2x16unorm),
            "unpack4x8snorm" => Some(Builtin::Unpack4x8snorm),
            "unpack4x8unorm" => Some(Builtin::Unpack4x8unorm),
            "unpack4xI8" => Some(Builtin::Unpack4xI8),
            "unpack4xU8" => Some(Builtin::Unpack4xU8),
            "subgroupAdd" => Some(Builtin::SubgroupAdd),
            "subgroupExclusiveAdd" => Some(Builtin::SubgroupExclusiveAdd),
            "subgroupInclusiveAdd" => Some(Builtin::SubgroupInclusiveAdd),
            "subgroupAll" => Some(Builtin::SubgroupAll),
            "subgroupAny" => Some(Builtin::SubgroupAny),
            "subgroupBallot" => Some(Builtin::SubgroupBallot),
            "subgroupBroadcast" => Some(Builtin::SubgroupBroadcast),
            "subgroupBroadcastFirst" => Some(Builtin::SubgroupBroadcastFirst),
            "subgroupElect" => Some(Builtin::SubgroupElect),
            "subgroupMax" => Some(Builtin::SubgroupMax),
            "subgroupMin" => Some(Builtin::SubgroupMin),
            "subgroupMul" => Some(Builtin::SubgroupMul),
            "subgroupExclusiveMul" => Some(Builtin::SubgroupExclusiveMul),
            "subgroupInclusiveMul" => Some(Builtin::SubgroupInclusiveMul),
            "subgroupOr" => Some(Builtin::SubgroupOr),
            "subgroupShuffle" => Some(Builtin::SubgroupShuffle),
            "subgroupShuffleDown" => Some(Builtin::SubgroupShuffleDown),
            "subgroupShuffleUp" => Some(Builtin::SubgroupShuffleUp),
            "subgroupShuffleXor" => Some(Builtin::SubgroupShuffleXor),
            "subgroupXor" => Some(Builtin::SubgroupXor),
            "quadBroadcast" => Some(Builtin::QuadBroadcast),
            "quadSwapDiagonal" => Some(Builtin::QuadSwapDiagonal),
            "quadSwapX" => Some(Builtin::QuadSwapX),
            "quadSwapY" => Some(Builtin::QuadSwapY),
            "workgroupUniformLoad" => Some(Builtin::WorkgroupUniformLoad),
            _ => None,
        }
    }
}

pub fn evaluate_builtin(ident: &Builtin, args: Vec<Option<Value>>) -> Option<Value> {
    // evaluate based on number of arguments passed to builtin
    match ident {
        Builtin::InsertBits => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            let arg3 = args[2].clone().unwrap();
            let arg4 = args[3].clone().unwrap();

            evaluate_four_arg_builtin(ident, arg1, arg2, arg3, arg4)
        }

        Builtin::Select
        | Builtin::ExtractBits
        | Builtin::Clamp
        | Builtin::Smoothstep
        | Builtin::Fma
        | Builtin::Mix => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            let arg3 = args[2].clone().unwrap();

            evaluate_three_arg_builtin(ident, arg1, arg2, arg3)
        }

        Builtin::Min
        | Builtin::Max
        | Builtin::Dot4I8Packed
        | Builtin::Dot4U8Packed
        | Builtin::Step
        | Builtin::Pow
        | Builtin::Ldexp => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();

            evaluate_two_arg_builtin(ident, arg1, arg2)
        }

        Builtin::Atan2 => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            evaluate_two_arg_builtin(ident, arg1, arg2)
        }

        Builtin::Normalize => {
            let arg = args[0].clone().unwrap();
            evaluate_normalize(arg)
        }

        Builtin::Transpose => {
            let arg = args[0].clone().unwrap();
            evaluate_transpose(arg)
        }

        Builtin::Length => {
            let arg = args[0].clone().unwrap();
            evaluate_length(arg)
        }

        Builtin::Distance => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            evaluate_distance(arg1, arg2)
        }

        Builtin::Cross => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            evaluate_cross(arg1, arg2)
        }

        Builtin::Reflect => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            evaluate_reflect(arg1, arg2)
        }

        Builtin::FaceForward => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            let arg3 = args[2].clone().unwrap();
            evaluate_face_forward(arg1, arg2, arg3)
        }

        Builtin::Refract => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();
            let arg3 = args[2].clone().unwrap();
            evaluate_refract(arg1, arg2, arg3)
        }

        Builtin::Dot => {
            let arg1 = args[0].clone().unwrap();
            let arg2 = args[1].clone().unwrap();

            evaluate_dot(arg1, arg2)
        }

        // these are reductions (Vector -> Scalar), so we can't use evaluate_single_arg_builtin
        Builtin::All | Builtin::Any => {
            let arg = args[0].clone().unwrap();
            evaluate_bool_reduction(ident, arg)
        }

        Builtin::Pack4x8snorm
        | Builtin::Pack4x8unorm
        | Builtin::Pack4xI8
        | Builtin::Pack4xU8
        | Builtin::Pack4xI8Clamp
        | Builtin::Pack4xU8Clamp
        | Builtin::Pack2x16snorm
        | Builtin::Pack2x16unorm
        | Builtin::Pack2x16float => {
            let arg = args[0].clone().unwrap();
            evaluate_pack(ident, arg)
        }
        Builtin::Unpack4x8snorm
        | Builtin::Unpack4x8unorm
        | Builtin::Unpack4xI8
        | Builtin::Unpack4xU8
        | Builtin::Unpack2x16snorm
        | Builtin::Unpack2x16unorm
        | Builtin::Unpack2x16float => {
            let arg = args[0].clone().unwrap();
            evaluate_unpack(ident, arg)
        }
        Builtin::Determinant => {
            let arg = args[0].clone().unwrap();
            evaluate_determinant(arg)
        }
        Builtin::SubgroupAdd
        | Builtin::SubgroupExclusiveAdd
        | Builtin::SubgroupInclusiveAdd
        | Builtin::SubgroupAll
        | Builtin::SubgroupAny
        | Builtin::SubgroupBallot
        | Builtin::SubgroupBroadcast
        | Builtin::SubgroupBroadcastFirst
        | Builtin::SubgroupElect
        | Builtin::SubgroupMax
        | Builtin::SubgroupMin
        | Builtin::SubgroupMul
        | Builtin::SubgroupExclusiveMul
        | Builtin::SubgroupInclusiveMul
        | Builtin::SubgroupOr
        | Builtin::SubgroupShuffle
        | Builtin::SubgroupShuffleDown
        | Builtin::SubgroupShuffleUp
        | Builtin::SubgroupShuffleXor
        | Builtin::SubgroupXor
        | Builtin::QuadBroadcast
        | Builtin::QuadSwapDiagonal
        | Builtin::QuadSwapX
        | Builtin::QuadSwapY
        | Builtin::WorkgroupUniformLoad => None,

        _ => {
            let single_arg = args[0].clone().unwrap();
            evaluate_single_arg_builtin(ident, single_arg)
        }
    }
}

fn evaluate_single_arg_builtin(ident: &Builtin, arg: Value) -> Option<Value> {
    match arg {
        Value::Lit(val) => evaluate(ident, val),
        Value::Vector(val) => {
            let mut result = Vec::new();

            for v in val {
                let elem = evaluate_single_arg_builtin(ident, v);

                match elem {
                    Some(e) => result.push(e),
                    None => {
                        return None;
                    }
                }
            }

            Some(Value::Vector(result))
        }
    }
}

fn evaluate_two_arg_builtin(ident: &Builtin, arg1: Value, arg2: Value) -> Option<Value> {
    match (arg1, arg2) {
        (Value::Lit(val1), Value::Lit(val2)) => evaluate_two_args(ident, val1, val2),
        (Value::Vector(val1), Value::Vector(val2)) => {
            let mut result = Vec::new();

            for (x, y) in val1.into_iter().zip(val2) {
                let elem = evaluate_two_arg_builtin(ident, x, y);

                match elem {
                    Some(e) => result.push(e),
                    None => {
                        return None;
                    }
                }
            }

            Some(Value::Vector(result))
        }
        _ => todo!(), // cannot have mixed types in implemented builtin fn evaluation
    }
}

fn evaluate_three_arg_builtin(
    ident: &Builtin,
    arg1: Value,
    arg2: Value,
    arg3: Value,
) -> Option<Value> {
    match (arg1, arg2, arg3) {
        (Value::Lit(val1), Value::Lit(val2), Value::Lit(val3)) => {
            evaluate_three_args(ident, val1, val2, val3)
        }
        (Value::Vector(val1), Value::Vector(val2), Value::Vector(val3)) => {
            let mut result = Vec::new();

            for ((x, y), z) in val1.into_iter().zip(val2).zip(val3) {
                let elem = evaluate_three_arg_builtin(ident, x, y, z);

                match elem {
                    Some(e) => result.push(e),
                    None => {
                        return None;
                    }
                }
            }

            Some(Value::Vector(result))
        }
        (Value::Vector(val1), Value::Vector(val2), Value::Lit(val3)) => {
            let mut result = Vec::new();

            for (x, y) in val1.into_iter().zip(val2) {
                let elem = evaluate_three_arg_builtin(ident, x, y, Value::Lit(val3));

                match elem {
                    Some(e) => result.push(e),
                    None => {
                        return None;
                    }
                }
            }

            Some(Value::Vector(result))
        }
        _ => None,
    }
}

fn evaluate_four_arg_builtin(
    ident: &Builtin,
    arg1: Value,
    arg2: Value,
    arg3: Value,
    arg4: Value,
) -> Option<Value> {
    match (arg1, arg2, arg3, arg4) {
        (Value::Lit(val1), Value::Lit(val2), Value::Lit(val3), Value::Lit(val4)) => {
            evaluate_four_args(ident, val1, val2, val3, val4)
        }
        (Value::Vector(val1), Value::Vector(val2), Value::Vector(val3), Value::Vector(val4)) => {
            let mut result = Vec::new();
            for (((x, y), z), w) in val1
                .iter()
                .zip(val2.iter())
                .zip(val3.iter())
                .zip(val4.iter())
            {
                let elem =
                    evaluate_four_arg_builtin(ident, x.clone(), y.clone(), z.clone(), w.clone());

                match elem {
                    Some(e) => result.push(e),
                    None => return None,
                }
            }
            Some(Value::Vector(result))
        }
        _ => None,
    }
}

fn evaluate(ident: &Builtin, val: Lit) -> Option<Value> {
    match ident {
        Builtin::Exp => exp(val),
        Builtin::Exp2 => exp2(val),
        Builtin::Abs => abs(val),
        Builtin::Acos => acos(val),
        Builtin::Acosh => acosh(val),
        Builtin::Asin => asin(val),
        Builtin::Atanh => atanh(val),
        Builtin::CountOneBits => count_one_bits(val),
        Builtin::CountLeadingZeros => count_leading_zeros(val),
        Builtin::CountTrailingZeros => count_trailing_zeros(val),
        Builtin::ReverseBits => reverse_bits(val),
        Builtin::FirstLeadingBit => first_leading_bit(val),
        Builtin::FirstTrailingBit => first_trailing_bit(val),
        Builtin::Atan => atan(val),
        Builtin::Cos => cos(val),
        Builtin::Cosh => cosh(val),
        Builtin::Degrees => degrees(val),
        Builtin::InverseSqrt => inverse_sqrt(val),
        Builtin::Log => log(val),
        Builtin::Log2 => log2(val),
        Builtin::Radians => radians(val),
        Builtin::Sin => sin(val),
        Builtin::Sinh => sinh(val),
        Builtin::Sqrt => sqrt(val),
        Builtin::Tan => tan(val),
        Builtin::Tanh => tanh(val),
        Builtin::Fract => fract(val),
        Builtin::Ceil => ceil(val),
        Builtin::Floor => floor(val),
        Builtin::Trunc => trunc(val),
        Builtin::Round => round(val),
        Builtin::Saturate => saturate(val),
        Builtin::Sign => sign(val),
        Builtin::QuantizeToF16 => quantize_to_f16(val),
        _ => todo!(),
    }
}

fn evaluate_two_args(ident: &Builtin, val1: Lit, val2: Lit) -> Option<Value> {
    match ident {
        Builtin::Min => min(val1, val2),
        Builtin::Max => max(val1, val2),
        Builtin::Atan2 => atan2(val1, val2),
        Builtin::Pow => pow(val1, val2),
        Builtin::Step => step(val1, val2),
        Builtin::Ldexp => ldexp(val1, val2),
        Builtin::Dot4I8Packed => {
            if let (Lit::U32(a), Lit::U32(b)) = (val1, val2) {
                let mut sum = 0i32;
                for i in 0..4 {
                    let c1 = ((a >> (8 * i)) & 0xFF) as u8 as i8 as i32;
                    let c2 = ((b >> (8 * i)) & 0xFF) as u8 as i8 as i32;
                    sum += c1 * c2;
                }
                Some(Value::Lit(Lit::I32(sum)))
            } else {
                panic!("Dot4I8Packed requires u32 arguments")
            }
        }
        Builtin::Dot4U8Packed => {
            if let (Lit::U32(a), Lit::U32(b)) = (val1, val2) {
                let mut sum = 0u32;
                for i in 0..4 {
                    let c1 = (a >> (8 * i)) & 0xFF;
                    let c2 = (b >> (8 * i)) & 0xFF;
                    sum += c1 * c2;
                }
                Some(Value::Lit(Lit::U32(sum)))
            } else {
                panic!("Dot4U8Packed requires u32 arguments")
            }
        }
        _ => todo!(),
    }
}

fn evaluate_three_args(ident: &Builtin, val1: Lit, val2: Lit, val3: Lit) -> Option<Value> {
    match ident {
        Builtin::Select => select(val1, val2, val3),
        Builtin::ExtractBits => extract_bits(val1, val2, val3),
        Builtin::Clamp => clamp(val1, val2, val3),
        Builtin::Smoothstep => smoothstep(val1, val2, val3),
        Builtin::Fma => fma(val1, val2, val3),
        Builtin::Mix => mix(val1, val2, val3),
        _ => todo!(),
    }
}

fn evaluate_four_args(
    ident: &Builtin,
    val1: Lit,
    val2: Lit,
    val3: Lit,
    val4: Lit,
) -> Option<Value> {
    match ident {
        Builtin::InsertBits => insert_bits(val1, val2, val3, val4),
        _ => todo!(),
    }
}

fn evaluate_bool_reduction(ident: &Builtin, arg: Value) -> Option<Value> {
    match arg {
        Value::Vector(vec) => {
            // If the vector contains anything other than bools, it's invalid WGSL.
            let mut bool_values = Vec::with_capacity(vec.len());

            for v in vec {
                match v {
                    Value::Lit(Lit::Bool(b)) => bool_values.push(b),
                    _ => return None,
                }
            }

            let result = match ident {
                Builtin::Any => bool_values.into_iter().any(|b| b),
                Builtin::All => bool_values.into_iter().all(|b| b),
                _ => return None,
            };

            Some(result.into())
        }
        Value::Lit(Lit::Bool(b)) => Some(b.into()),
        _ => None,
    }
}

fn evaluate_pack(ident: &Builtin, arg: Value) -> Option<Value> {
    let vec = match arg {
        Value::Vector(v) => v,
        _ => return None,
    };

    match ident {
        Builtin::Pack4x8snorm => {
            if vec.len() != 4 {
                panic!("Pack4x8snorm requires a vec4")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::F32(f)) = v {
                    let clamped = f.clamp(-1.0, 1.0);
                    let val = (0.5 + 127.0 * clamped).floor() as i32;
                    let val_u8 = val as u8 as u32;
                    res |= val_u8 << (8 * i);
                } else {
                    panic!("Pack4x8snorm requires f32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack4x8unorm => {
            if vec.len() != 4 {
                panic!("Pack4x8unorm requires a vec4")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::F32(f)) = v {
                    let clamped = f.clamp(0.0, 1.0);
                    let val = (0.5 + 255.0 * clamped).floor() as u32;
                    let val_u8 = val as u8 as u32;
                    res |= val_u8 << (8 * i);
                } else {
                    panic!("Pack4x8unorm requires f32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack4xI8 => {
            if vec.len() != 4 {
                panic!("Pack4xI8 requires a vec4")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::I32(val)) = v {
                    let val_u8 = (*val as u8) as u32;
                    res |= val_u8 << (8 * i);
                } else {
                    panic!("Pack4xI8 requires i32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack4xU8 => {
            if vec.len() != 4 {
                panic!("Pack4xU8 requires a vec4")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::U32(val)) = v {
                    let val_u8 = (*val as u8) as u32;
                    res |= val_u8 << (8 * i);
                } else {
                    panic!("Pack4xU8 requires u32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack4xI8Clamp => {
            if vec.len() != 4 {
                panic!("Pack4xI8Clamp requires a vec4")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::I32(val)) = v {
                    let clamped = (*val).clamp(-128, 127);
                    let val_u8 = (clamped as u8) as u32;
                    res |= val_u8 << (8 * i);
                } else {
                    panic!("Pack4xI8Clamp requires i32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack4xU8Clamp => {
            if vec.len() != 4 {
                panic!("Pack4xU8Clamp requires a vec4")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::U32(val)) = v {
                    let clamped = (*val).min(255);
                    let val_u8 = (clamped as u8) as u32;
                    res |= val_u8 << (8 * i);
                } else {
                    panic!("Pack4xU8Clamp requires u32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack2x16snorm => {
            if vec.len() != 2 {
                panic!("Pack2x16snorm requires a vec2")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::F32(f)) = v {
                    let clamped = f.clamp(-1.0, 1.0);
                    let val = (0.5 + 32767.0 * clamped).floor() as i32;
                    let val_u16 = val as u16 as u32;
                    res |= val_u16 << (16 * i);
                } else {
                    panic!("Pack2x16snorm requires f32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack2x16unorm => {
            if vec.len() != 2 {
                panic!("Pack2x16unorm requires 2 arguments")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::F32(f)) = v {
                    let clamped = f.clamp(0.0, 1.0);
                    let val = (0.5 + 65535.0 * clamped).floor() as u32;
                    let val_u16 = val as u16 as u32;
                    res |= val_u16 << (16 * i);
                } else {
                    panic!("Pack2x16unorm requires f32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        Builtin::Pack2x16float => {
            if vec.len() != 2 {
                panic!("Pack2x16float requires a vec2")
            }
            let mut res = 0u32;
            for (i, v) in vec.iter().enumerate() {
                if let Value::Lit(Lit::F32(f)) = v {
                    if f.abs() > 65504.0 || f.is_nan() {
                        return None;
                    }
                    let f16_val = half::f16::from_f32(*f);
                    let val_u16 = f16_val.to_bits() as u32;
                    res |= val_u16 << (16 * i);
                } else {
                    panic!("Pack2x16float requires f32 arguments")
                }
            }
            Some(Value::Lit(Lit::U32(res)))
        }
        _ => None,
    }
}

fn evaluate_unpack(ident: &Builtin, arg: Value) -> Option<Value> {
    let val = match arg {
        Value::Lit(Lit::U32(v)) => v,
        _ => return None,
    };

    match ident {
        Builtin::Unpack4x8snorm => {
            let mut res = Vec::new();
            for i in 0..4 {
                let chunk = ((val >> (8 * i)) & 0xFF) as u8 as i8 as i32;
                let f = (chunk as f32 / 127.0).max(-1.0);
                res.push(Value::Lit(Lit::F32(f)));
            }
            Some(Value::Vector(res))
        }
        Builtin::Unpack4x8unorm => {
            let mut res = Vec::new();
            for i in 0..4 {
                let chunk = (val >> (8 * i)) & 0xFF;
                let f = chunk as f32 / 255.0;
                res.push(Value::Lit(Lit::F32(f)));
            }
            Some(Value::Vector(res))
        }
        Builtin::Unpack4xI8 => {
            let mut res = Vec::new();
            for i in 0..4 {
                let chunk = ((val >> (8 * i)) & 0xFF) as u8 as i8 as i32;
                res.push(Value::Lit(Lit::I32(chunk)));
            }
            Some(Value::Vector(res))
        }
        Builtin::Unpack4xU8 => {
            let mut res = Vec::new();
            for i in 0..4 {
                let chunk = (val >> (8 * i)) & 0xFF;
                res.push(Value::Lit(Lit::U32(chunk)));
            }
            Some(Value::Vector(res))
        }
        Builtin::Unpack2x16snorm => {
            let mut res = Vec::new();
            for i in 0..2 {
                let chunk = ((val >> (16 * i)) & 0xFFFF) as u16 as i16 as i32;
                let f = (chunk as f32 / 32767.0).max(-1.0);
                res.push(Value::Lit(Lit::F32(f)));
            }
            Some(Value::Vector(res))
        }
        Builtin::Unpack2x16unorm => {
            let mut res = Vec::new();
            for i in 0..2 {
                let chunk = (val >> (16 * i)) & 0xFFFF;
                let f = chunk as f32 / 65535.0;
                res.push(Value::Lit(Lit::F32(f)));
            }
            Some(Value::Vector(res))
        }
        Builtin::Unpack2x16float => {
            let mut res = Vec::new();
            for i in 0..2 {
                let chunk = ((val >> (16 * i)) & 0xFFFF) as u16;
                let f = half::f16::from_bits(chunk).to_f32();
                if !f.is_finite() {
                    return None;
                }
                res.push(Value::Lit(Lit::F32(f)));
            }
            Some(Value::Vector(res))
        }
        _ => None,
    }
}

fn evaluate_determinant(arg: Value) -> Option<Value> {
    let cols = match arg {
        Value::Vector(v) => v,
        _ => return None,
    };
    let n = cols.len();
    if !(2..=4).contains(&n) {
        return None;
    }
    let is_f16 = match &cols[0] {
        Value::Vector(col) => match &col[0] {
            Value::Lit(Lit::F32(_)) => false,
            Value::Lit(Lit::F16(_)) => true,
            _ => return None,
        },
        _ => return None,
    };

    let mut m = [[0.0f32; 4]; 4];
    for (c, col) in cols.iter().enumerate() {
        if let Value::Vector(col_vec) = col {
            if col_vec.len() != n {
                return None;
            }
            for (r, val) in col_vec.iter().enumerate() {
                match val {
                    Value::Lit(Lit::F32(f)) => m[c][r] = *f,
                    Value::Lit(Lit::F16(f)) => m[c][r] = f.to_f32(),
                    _ => return None,
                }
            }
        } else {
            return None;
        }
    }

    let check = |f: f32| -> Option<f32> {
        if is_f16 {
            in_float16_range(half::f16::from_f32(f)).map(|x| x.to_f32())
        } else {
            in_float_range(f)
        }
    };

    let mul = |a: f32, b: f32| check(a * b);
    let add = |a: f32, b: f32| check(a + b);
    let sub = |a: f32, b: f32| check(a - b);

    let det2 = |c0: usize, c1: usize, r0: usize, r1: usize| -> Option<f32> {
        sub(mul(m[c0][r0], m[c1][r1])?, mul(m[c1][r0], m[c0][r1])?)
    };

    let det3 = |c0: usize, c1: usize, c2: usize, r0: usize, r1: usize, r2: usize| -> Option<f32> {
        let a = mul(m[c0][r0], det2(c1, c2, r1, r2)?)?;
        let b = mul(m[c1][r0], det2(c0, c2, r1, r2)?)?;
        let c = mul(m[c2][r0], det2(c0, c1, r1, r2)?)?;
        add(sub(a, b)?, c)
    };

    let det = match n {
        2 => det2(0, 1, 0, 1)?,
        3 => det3(0, 1, 2, 0, 1, 2)?,
        4 => {
            let a = mul(m[0][0], det3(1, 2, 3, 1, 2, 3)?)?;
            let b = mul(m[1][0], det3(0, 2, 3, 1, 2, 3)?)?;
            let c = mul(m[2][0], det3(0, 1, 3, 1, 2, 3)?)?;
            let d = mul(m[3][0], det3(0, 1, 2, 1, 2, 3)?)?;
            sub(add(sub(a, b)?, c)?, d)?
        }
        _ => return None,
    };

    if is_f16 {
        Some(Value::Lit(Lit::F16(half::f16::from_f32(det))))
    } else {
        Some(Value::Lit(Lit::F32(det)))
    }
}

fn count_one_bits(val: Lit) -> Option<Value> {
    match val {
        Lit::I32(v) => Value::from_i32(i32::try_from(v.count_ones()).ok()),
        Lit::U32(v) => Value::from_u32(Some(v.count_ones())),
        _ => None,
    }
}

fn count_leading_zeros(val: Lit) -> Option<Value> {
    match val {
        Lit::I32(v) => Value::from_i32(Some(v.leading_zeros() as i32)),
        Lit::U32(v) => Value::from_u32(Some(v.leading_zeros())),
        _ => None,
    }
}

fn count_trailing_zeros(val: Lit) -> Option<Value> {
    match val {
        Lit::I32(v) => Value::from_i32(Some(v.trailing_zeros() as i32)),
        Lit::U32(v) => Value::from_u32(Some(v.trailing_zeros())),
        _ => None,
    }
}

fn extract_bits(val: Lit, offset_arg: Lit, count_arg: Lit) -> Option<Value> {
    // offset and count must be u32
    let offset = match offset_arg {
        Lit::U32(u) => u,
        _ => return None,
    };
    let count = match count_arg {
        Lit::U32(u) => u,
        _ => return None,
    };

    if (offset as u64) + (count as u64) > 32 {
        return None;
    }

    // If count is 0, result is 0
    if count == 0 {
        match val {
            Lit::I32(_) => return Some(0.into()),
            Lit::U32(_) => return Some(0.into()),
            _ => return None,
        }
    }

    match val {
        Lit::I32(v) => {
            // Signed extract: Sign-extend from the (count-1)th bit of the result.
            // Algorithm: Shift left to clear upper bits, arithmetic shift right to restore position and sign-extend.
            let shift_left = 32 - (offset + count);
            let shift_right = 32 - count;

            let result = (v << shift_left) >> shift_right;
            Some(result.into())
        }
        Lit::U32(v) => {
            // Unsigned extract: Zero-extend.
            // Algorithm: Shift right to move to LSB, mask out upper bits.
            let shifted = v >> offset;
            // Handle count=32 carefully to avoid overflow in mask generation (1 << 32)
            let mask = if count == 32 {
                u32::MAX
            } else {
                (1 << count) - 1
            };

            Some((shifted & mask).into())
        }
        _ => None,
    }
}

fn clamp(e: Lit, low: Lit, high: Lit) -> Option<Value> {
    match (e, low, high) {
        (Lit::I32(e_val), Lit::I32(low_val), Lit::I32(high_val)) => {
            if low_val > high_val {
                return None;
            }
            Some(e_val.clamp(low_val, high_val).into())
        }
        (Lit::U32(e_val), Lit::U32(low_val), Lit::U32(high_val)) => {
            if low_val > high_val {
                return None;
            }
            Some(e_val.clamp(low_val, high_val).into())
        }
        (Lit::F32(e_val), Lit::F32(low_val), Lit::F32(high_val)) => {
            if low_val > high_val {
                return None;
            }
            Some(e_val.clamp(low_val, high_val).into())
        }
        (Lit::F16(e_val), Lit::F16(low_val), Lit::F16(high_val)) => {
            if low_val > high_val {
                return None;
            }
            Some(e_val.clamp(low_val, high_val).into())
        }
        _ => None,
    }
}

fn smoothstep(low: Lit, high: Lit, x: Lit) -> Option<Value> {
    match (low, high, x) {
        (Lit::F32(low_val), Lit::F32(high_val), Lit::F32(x_val)) => {
            if low_val == high_val {
                return None;
            }
            let diff = in_float_range(high_val - low_val)?;
            let diff_x = in_float_range(x_val - low_val)?;
            let mut t = in_float_range(diff_x / diff)?;
            t = t.clamp(0.0, 1.0);
            let t_sq = in_float_range(t * t)?;
            let two_t = in_float_range(2.0 * t)?;
            let three_minus = in_float_range(3.0 - two_t)?;
            let result = in_float_range(t_sq * three_minus)?;
            Some(result.into())
        }
        (Lit::F16(low_val), Lit::F16(high_val), Lit::F16(x_val)) => {
            if low_val == high_val {
                return None;
            }
            let l = low_val.to_f32();
            let h = high_val.to_f32();
            let x_f = x_val.to_f32();
            let diff = in_float16_range(half::f16::from_f32(h - l))?.to_f32();
            let diff_x = in_float16_range(half::f16::from_f32(x_f - l))?.to_f32();
            let mut t = in_float16_range(half::f16::from_f32(diff_x / diff))?.to_f32();
            t = t.clamp(0.0, 1.0);
            let t_sq = in_float16_range(half::f16::from_f32(t * t))?.to_f32();
            let two_t = in_float16_range(half::f16::from_f32(2.0 * t))?.to_f32();
            let three_minus = in_float16_range(half::f16::from_f32(3.0 - two_t))?.to_f32();
            let result = in_float16_range(half::f16::from_f32(t_sq * three_minus))?.to_f32();
            Some(half::f16::from_f32(result).into())
        }
        _ => None,
    }
}

fn insert_bits(e_arg: Lit, newbits_arg: Lit, offset_arg: Lit, count_arg: Lit) -> Option<Value> {
    let offset = match offset_arg {
        Lit::U32(u) => u,
        _ => return None,
    };
    let count = match count_arg {
        Lit::U32(u) => u,
        _ => return None,
    };

    if (offset as u64) + (count as u64) > 32 {
        return None;
    }

    if count == 0 {
        match e_arg {
            Lit::I32(v) => return Some(v.into()),
            Lit::U32(v) => return Some(v.into()),
            _ => return None,
        }
    }

    // Helper to perform the bitwise logic on raw u32 bits
    let calc_insert = |e_raw: u32, new_raw: u32| -> u32 {
        let mask_width = if count == 32 {
            u32::MAX
        } else {
            (1 << count) - 1
        };
        let mask = mask_width << offset;

        // 1. Clear the bits in e where we want to insert (e & !mask)
        // 2. Mask the newbits to count size and shift them up ((new & mask_width) << offset)
        // 3. OR them together
        (e_raw & !mask) | ((new_raw & mask_width) << offset)
    };

    match (e_arg, newbits_arg) {
        (Lit::I32(e), Lit::I32(newbits)) => {
            let result_raw = calc_insert(e as u32, newbits as u32);
            Some((result_raw as i32).into())
        }
        (Lit::U32(e), Lit::U32(newbits)) => {
            let result = calc_insert(e, newbits);
            Some(result.into())
        }
        _ => None,
    }
}

fn abs(val: Lit) -> Option<Value> {
    match val {
        Lit::I32(v) => Value::from_i32(Some(v.wrapping_abs())),
        Lit::F32(v) => Value::from_f32(Some(v.abs())),
        Lit::F16(v) => Value::from_f16(Some(half::f16::from_f32(v.to_f32().abs()))),
        Lit::U32(v) => Value::from_u32(Some(v)),
        _ => None,
    }
}

fn acos(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if !(-1.0..=1.0).contains(&v) {
                return None;
            }
            Value::from_f32(in_float_range(v.acos()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if !(-1.0..=1.0).contains(&f) {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.acos())))
        }
        _ => None,
    }
}

fn acosh(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if v < 1.0 || v.is_nan() {
                return None;
            }
            Value::from_f32(in_float_range(v.acosh()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if f < 1.0 || f.is_nan() {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.acosh())))
        }
        _ => None,
    }
}

fn asin(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if !(-1.0..=1.0).contains(&v) {
                return None;
            }
            Value::from_f32(in_float_range(v.asin()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if !(-1.0..=1.0).contains(&f) {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.asin())))
        }
        _ => None,
    }
}

fn atanh(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if !(-1.0..=1.0).contains(&v) {
                return None;
            }
            Value::from_f32(in_float_range(v.atanh()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if !(-1.0..=1.0).contains(&f) {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.atanh())))
        }
        _ => None,
    }
}

fn exp(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            // The maximum representable f32 is approx 3.4028e38.
            // ln(3.4028e38) approx 88.72.
            // If v > 88.72, exp(v) overflows f32.
            if v > 88.72_f32 {
                return None;
            }

            let result = in_float_range(v.exp());
            Value::from_f32(result)
        }
        Lit::F16(v) => {
            // max f16 is 65504. ln(65504) approx 11.09
            if v.to_f32() > 11.09_f32 {
                return None;
            }
            let result = in_float16_range(half::f16::from_f32(v.to_f32().exp()));
            Value::from_f16(result)
        }
        _ => None,
    }
}

fn exp2(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            // TODO: Can we replace this entire block with Value::from_f32(Some(v.exp2()))?

            // approximation - the maximum representable f32
            // is (2 - 2^-23)*2^127. so, conservatively if
            // v > 127 then exp2(v) is not representable as
            // a concrete f32. so replace node if v > 127
            if v > 127.0_f32 {
                return None;
            }

            // otherwise, if v < 127 we need to check that
            // the result is precisely representable. another
            // approximation is used to restrict f32 value to
            // the precicely representable range (as in
            // in_float_range(f32)
            let a = 2.0_f32;

            let result = in_float_range(a.powf(v));

            Value::from_f32(result)
        }
        Lit::F16(v) => {
            // max f16 is 65504. log2(65504) approx 15.99
            if v.to_f32() > 15.99_f32 {
                return None;
            }
            let result = in_float16_range(half::f16::from_f32(2.0_f32.powf(v.to_f32())));
            Value::from_f16(result)
        }
        _ => None,
    }
}
// TODO: move min/max/select etc to evaluator.rs or elsewhere?

fn min(val1: Lit, val2: Lit) -> Option<Value> {
    match (val1, val2) {
        (Lit::I32(v1), Lit::I32(v2)) => Some(v1.min(v2).into()),
        (Lit::U32(v1), Lit::U32(v2)) => Some(v1.min(v2).into()),
        (Lit::F32(v1), Lit::F32(v2)) => Some(v1.min(v2).into()),
        (Lit::F16(v1), Lit::F16(v2)) => Some(v1.min(v2).into()),
        _ => None,
    }
}

fn max(val1: Lit, val2: Lit) -> Option<Value> {
    match (val1, val2) {
        (Lit::I32(v1), Lit::I32(v2)) => Some(v1.max(v2).into()),
        (Lit::U32(v1), Lit::U32(v2)) => Some(v1.max(v2).into()),
        (Lit::F32(v1), Lit::F32(v2)) => Some(v1.max(v2).into()),
        (Lit::F16(v1), Lit::F16(v2)) => Some(v1.max(v2).into()),
        _ => None,
    }
}

fn select(val1: Lit, val2: Lit, val3: Lit) -> Option<Value> {
    // val1 = false case, val2 = true case, val3 = condition
    let cond = match val3 {
        Lit::Bool(b) => b,
        _ => return None,
    };

    match (val1, val2) {
        (Lit::I32(v1), Lit::I32(v2)) => Some(if cond { v2 } else { v1 }.into()),
        (Lit::U32(v1), Lit::U32(v2)) => Some(if cond { v2 } else { v1 }.into()),
        (Lit::F32(v1), Lit::F32(v2)) => Some(if cond { v2 } else { v1 }.into()),
        (Lit::F16(v1), Lit::F16(v2)) => Some(if cond { v2 } else { v1 }.into()),
        // (Lit::Bool(v1), Lit::Bool(v2)) => Some(if cond { v2 } else { v1 }.into()),
        _ => None,
    }
}

fn reverse_bits(val: Lit) -> Option<Value> {
    match val {
        Lit::I32(v) => Some(v.reverse_bits().into()),
        Lit::U32(v) => Some(v.reverse_bits().into()),
        _ => None,
    }
}

fn first_leading_bit(val: Lit) -> Option<Value> {
    match val {
        // signed: returns index of the first bit that differs from the sign bit
        // if value is 0 or -1, returns -1
        Lit::I32(v) => {
            // If v is negative, invert it.
            // This maps -1 (...111) to 0 (...000), making the logic identical to positive numbers.
            // i.e., finding the highest '1' of `!v` is finding the highest '0' in `v`.
            let effective_v = if v < 0 { !v } else { v };

            if effective_v == 0 {
                return Some((-1_i32).into());
            }

            let leading_zeros = effective_v.leading_zeros();
            let index = 31 - leading_zeros;
            Some((index as i32).into())
        }
        // unsigned: returns index of first set bit
        // if value is 0, returns -1 (u32::MAX)
        Lit::U32(v) => {
            if v == 0 {
                return Some(u32::MAX.into());
            }

            let index = 31 - v.leading_zeros();
            Some(index.into())
        }
        _ => None,
    }
}

fn first_trailing_bit(val: Lit) -> Option<Value> {
    match val {
        // signed: returns index of the least significant set bit
        // if value is 0, returns -1
        // Note: WGSL treats the bits as raw bits, so sign doesn't change the logic.
        Lit::I32(v) => {
            if v == 0 {
                return Some((-1_i32).into());
            }

            let index = v.trailing_zeros();
            Some((index as i32).into())
        }

        // unsigned: returns index of the least significant set bit
        // if value is 0, returns -1 (u32::MAX)
        Lit::U32(v) => {
            if v == 0 {
                return Some(u32::MAX.into());
            }

            let index = v.trailing_zeros();
            Some(index.into())
        }
        _ => None,
    }
}

fn evaluate_dot(arg1: Value, arg2: Value) -> Option<Value> {
    match (arg1, arg2) {
        (Value::Vector(v1), Value::Vector(v2)) => {
            if v1.len() != v2.len() || v1.is_empty() {
                return None;
            }

            match (&v1[0], &v2[0]) {
                (Value::Lit(Lit::I32(_)), Value::Lit(Lit::I32(_))) => {
                    let mut sum = 0i32;
                    for (x, y) in v1.iter().zip(v2.iter()) {
                        if let (Value::Lit(Lit::I32(xv)), Value::Lit(Lit::I32(yv))) = (x, y) {
                            let product = xv.wrapping_mul(*yv);
                            sum = sum.wrapping_add(product);
                        } else {
                            return None;
                        }
                    }
                    Some(sum.into())
                }
                (Value::Lit(Lit::U32(_)), Value::Lit(Lit::U32(_))) => {
                    let mut sum = 0u32;
                    for (x, y) in v1.iter().zip(v2.iter()) {
                        if let (Value::Lit(Lit::U32(xv)), Value::Lit(Lit::U32(yv))) = (x, y) {
                            let product = xv.wrapping_mul(*yv);
                            sum = sum.wrapping_add(product);
                        } else {
                            return None;
                        }
                    }
                    Some(sum.into())
                }
                (Value::Lit(Lit::F32(_)), Value::Lit(Lit::F32(_))) => {
                    let mut sum = 0.0f32;
                    for (x, y) in v1.iter().zip(v2.iter()) {
                        if let (Value::Lit(Lit::F32(xv)), Value::Lit(Lit::F32(yv))) = (x, y) {
                            let product = xv * yv;
                            in_float_range(product)?;
                            sum += product;
                            in_float_range(sum)?;
                        } else {
                            return None;
                        }
                    }
                    Some(sum.into())
                }
                (Value::Lit(Lit::F16(_)), Value::Lit(Lit::F16(_))) => {
                    let mut sum = 0.0f32;
                    for (x, y) in v1.iter().zip(v2.iter()) {
                        if let (Value::Lit(Lit::F16(xv)), Value::Lit(Lit::F16(yv))) = (x, y) {
                            let product = xv.to_f32() * yv.to_f32();
                            in_float16_range(half::f16::from_f32(product))?;
                            sum += product;
                            in_float16_range(half::f16::from_f32(sum))?;
                        } else {
                            return None;
                        }
                    }
                    Some(Value::Lit(Lit::F16(half::f16::from_f32(sum))))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

fn atan(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.atan())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().atan()))),
        _ => None,
    }
}

fn cos(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.cos())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().cos()))),
        _ => None,
    }
}

fn cosh(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.cosh())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().cosh()))),
        _ => None,
    }
}

fn degrees(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.to_degrees())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(
            v.to_f32().to_degrees(),
        ))),
        _ => None,
    }
}

fn inverse_sqrt(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if v <= 0.0 {
                return None;
            }
            Value::from_f32(in_float_range(1.0 / v.sqrt()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if f <= 0.0 {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(1.0 / f.sqrt())))
        }
        _ => None,
    }
}

fn log(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if v <= 0.0 {
                return None;
            }
            Value::from_f32(in_float_range(v.ln()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if f <= 0.0 {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.ln())))
        }
        _ => None,
    }
}

fn log2(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if v <= 0.0 {
                return None;
            }
            Value::from_f32(in_float_range(v.log2()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if f <= 0.0 {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.log2())))
        }
        _ => None,
    }
}

fn radians(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.to_radians())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(
            v.to_f32().to_radians(),
        ))),
        _ => None,
    }
}

fn sin(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.sin())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().sin()))),
        _ => None,
    }
}

fn sinh(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.sinh())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().sinh()))),
        _ => None,
    }
}

fn sqrt(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            if v < 0.0 {
                return None;
            }
            Value::from_f32(in_float_range(v.sqrt()))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            if f < 0.0 {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(f.sqrt())))
        }
        _ => None,
    }
}

fn tan(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.tan())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().tan()))),
        _ => None,
    }
}

fn tanh(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.tanh())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().tanh()))),
        _ => None,
    }
}

fn fract(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            let fl = in_float_range(v.floor())?;
            Value::from_f32(in_float_range(v - fl))
        }
        Lit::F16(v) => {
            let f = v.to_f32();
            let fl = in_float16_range(half::f16::from_f32(f.floor()))?.to_f32();
            Value::from_f16(in_float16_range(half::f16::from_f32(f - fl)))
        }
        _ => None,
    }
}

fn ceil(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.ceil())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().ceil()))),
        _ => None,
    }
}

fn floor(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.floor())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().floor()))),
        _ => None,
    }
}

fn trunc(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.trunc())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().trunc()))),
        _ => None,
    }
}

fn round(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => Value::from_f32(in_float_range(v.round())),
        Lit::F16(v) => Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().round()))),
        _ => None,
    }
}

fn saturate(val: Lit) -> Option<Value> {
    clamp(val, Lit::F32(0.0), Lit::F32(1.0)).or_else(|| {
        clamp(
            val,
            Lit::F16(half::f16::from_f32(0.0)),
            Lit::F16(half::f16::from_f32(1.0)),
        )
    })
}

fn sign(val: Lit) -> Option<Value> {
    match val {
        Lit::I32(v) => Value::from_i32(Some(v.signum())),
        Lit::F32(v) => Value::from_f32(Some(if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        })),
        Lit::F16(v) => Value::from_f16(Some(half::f16::from_f32(if v.to_f32() > 0.0 {
            1.0
        } else if v.to_f32() < 0.0 {
            -1.0
        } else {
            0.0
        }))),
        _ => None,
    }
}

fn quantize_to_f16(val: Lit) -> Option<Value> {
    match val {
        Lit::F32(v) => {
            let f16_val = half::f16::from_f32(v);
            if f16_val.is_infinite() || f16_val.is_nan() {
                return None;
            }
            Value::from_f32(Some(f16_val.to_f32()))
        }
        _ => None,
    }
}

fn pow(val1: Lit, val2: Lit) -> Option<Value> {
    match (val1, val2) {
        (Lit::F32(x), Lit::F32(y)) => {
            if x < 0.0 {
                return None;
            }
            if x == 0.0 && y <= 0.0 {
                return None;
            }
            Value::from_f32(in_float_range(x.powf(y)))
        }
        (Lit::F16(x), Lit::F16(y)) => {
            let xf = x.to_f32();
            let yf = y.to_f32();
            if xf < 0.0 {
                return None;
            }
            if xf == 0.0 && yf <= 0.0 {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(xf.powf(yf))))
        }
        _ => None,
    }
}

fn step(edge: Lit, x: Lit) -> Option<Value> {
    match (edge, x) {
        (Lit::F32(e), Lit::F32(v)) => Value::from_f32(Some(if e <= v { 1.0 } else { 0.0 })),
        (Lit::F16(e), Lit::F16(v)) => {
            Value::from_f16(Some(half::f16::from_f32(if e.to_f32() <= v.to_f32() {
                1.0
            } else {
                0.0
            })))
        }
        _ => None,
    }
}

fn ldexp(e1: Lit, e2: Lit) -> Option<Value> {
    match (e1, e2) {
        (Lit::F32(v), Lit::I32(exp)) => {
            if !(-127..=128).contains(&exp) {
                return None;
            }
            Value::from_f32(in_float_range(v * 2.0_f32.powi(exp)))
        }
        (Lit::F16(v), Lit::I32(exp)) => {
            if !(-15..=15).contains(&exp) {
                return None;
            }
            Value::from_f16(in_float16_range(half::f16::from_f32(
                v.to_f32() * 2.0_f32.powi(exp),
            )))
        }
        _ => None,
    }
}

fn fma(e1: Lit, e2: Lit, e3: Lit) -> Option<Value> {
    match (e1, e2, e3) {
        (Lit::F32(a), Lit::F32(b), Lit::F32(c)) => {
            let p = in_float_range(a * b)?;
            Value::from_f32(in_float_range(p + c))
        }
        (Lit::F16(a), Lit::F16(b), Lit::F16(c)) => {
            let p = in_float16_range(half::f16::from_f32(a.to_f32() * b.to_f32()))?.to_f32();
            Value::from_f16(in_float16_range(half::f16::from_f32(p + c.to_f32())))
        }
        _ => None,
    }
}

fn mix(e1: Lit, e2: Lit, e3: Lit) -> Option<Value> {
    match (e1, e2, e3) {
        (Lit::F32(a), Lit::F32(b), Lit::F32(c)) => {
            let one_minus_c = in_float_range(1.0 - c)?;
            let p1 = in_float_range(a * one_minus_c)?;
            let p2 = in_float_range(b * c)?;
            Value::from_f32(in_float_range(p1 + p2))
        }
        (Lit::F16(a), Lit::F16(b), Lit::F16(c)) => {
            let a = a.to_f32();
            let b = b.to_f32();
            let c = c.to_f32();

            let one_minus_c = in_float16_range(half::f16::from_f32(1.0 - c))?.to_f32();
            let p1 = in_float16_range(half::f16::from_f32(a * one_minus_c))?.to_f32();
            let p2 = in_float16_range(half::f16::from_f32(b * c))?.to_f32();
            Value::from_f16(in_float16_range(half::f16::from_f32(p1 + p2)))
        }
        _ => None,
    }
}

fn evaluate_length(arg: Value) -> Option<Value> {
    match arg {
        Value::Lit(Lit::F32(v)) => Value::from_f32(in_float_range(v.abs())),
        Value::Lit(Lit::F16(v)) => {
            Value::from_f16(in_float16_range(half::f16::from_f32(v.to_f32().abs())))
        }
        Value::Vector(vec) => {
            let mut sum = 0.0;
            let mut is_f16 = false;
            for v in vec {
                match v {
                    Value::Lit(Lit::F32(f)) => {
                        let sq = in_float_range(f * f)?;
                        sum = in_float_range(sum + sq)?;
                    }
                    Value::Lit(Lit::F16(f)) => {
                        let sq = in_float16_range(half::f16::from_f32(f.to_f32() * f.to_f32()))?
                            .to_f32();
                        sum = in_float16_range(half::f16::from_f32(sum + sq))?.to_f32();
                        is_f16 = true;
                    }
                    _ => return None,
                }
            }
            if is_f16 {
                Value::from_f16(in_float16_range(half::f16::from_f32(sum.sqrt())))
            } else {
                Value::from_f32(in_float_range(sum.sqrt()))
            }
        }
        _ => None,
    }
}

fn evaluate_distance(arg1: Value, arg2: Value) -> Option<Value> {
    match (arg1, arg2) {
        (Value::Lit(Lit::F32(a)), Value::Lit(Lit::F32(b))) => {
            Value::from_f32(in_float_range((a - b).abs()))
        }
        (Value::Lit(Lit::F16(a)), Value::Lit(Lit::F16(b))) => Value::from_f16(in_float16_range(
            half::f16::from_f32((a.to_f32() - b.to_f32()).abs()),
        )),
        (Value::Vector(v1), Value::Vector(v2)) => {
            if v1.len() != v2.len() {
                return None;
            }
            let mut sum = 0.0;
            let mut is_f16 = false;
            for (a, b) in v1.into_iter().zip(v2) {
                match (a, b) {
                    (Value::Lit(Lit::F32(fa)), Value::Lit(Lit::F32(fb))) => {
                        let diff = in_float_range(fa - fb)?;
                        let sq = in_float_range(diff * diff)?;
                        sum = in_float_range(sum + sq)?;
                    }
                    (Value::Lit(Lit::F16(fa)), Value::Lit(Lit::F16(fb))) => {
                        let diff =
                            in_float16_range(half::f16::from_f32(fa.to_f32() - fb.to_f32()))?
                                .to_f32();
                        let sq = in_float16_range(half::f16::from_f32(diff * diff))?.to_f32();
                        sum = in_float16_range(half::f16::from_f32(sum + sq))?.to_f32();
                        is_f16 = true;
                    }
                    _ => return None,
                }
            }
            if is_f16 {
                Value::from_f16(in_float16_range(half::f16::from_f32(sum.sqrt())))
            } else {
                Value::from_f32(in_float_range(sum.sqrt()))
            }
        }
        _ => None,
    }
}

fn evaluate_cross(arg1: Value, arg2: Value) -> Option<Value> {
    match (arg1, arg2) {
        (Value::Vector(v1), Value::Vector(v2)) => {
            if v1.len() != 3 || v2.len() != 3 {
                return None;
            }
            let (x1, y1, z1, is_f16) = match (&v1[0], &v1[1], &v1[2]) {
                (Value::Lit(Lit::F32(x)), Value::Lit(Lit::F32(y)), Value::Lit(Lit::F32(z))) => {
                    (*x, *y, *z, false)
                }
                (Value::Lit(Lit::F16(x)), Value::Lit(Lit::F16(y)), Value::Lit(Lit::F16(z))) => {
                    (x.to_f32(), y.to_f32(), z.to_f32(), true)
                }
                _ => return None,
            };
            let (x2, y2, z2) = match (&v2[0], &v2[1], &v2[2]) {
                (Value::Lit(Lit::F32(x)), Value::Lit(Lit::F32(y)), Value::Lit(Lit::F32(z))) => {
                    (*x, *y, *z)
                }
                (Value::Lit(Lit::F16(x)), Value::Lit(Lit::F16(y)), Value::Lit(Lit::F16(z))) => {
                    (x.to_f32(), y.to_f32(), z.to_f32())
                }
                _ => return None,
            };

            let check = |f: f32| -> Option<f32> {
                if is_f16 {
                    in_float16_range(half::f16::from_f32(f)).map(|x| x.to_f32())
                } else {
                    in_float_range(f)
                }
            };

            let mul = |a: f32, b: f32| check(a * b);
            let sub = |a: f32, b: f32| check(a - b);

            let rx = sub(mul(y1, z2)?, mul(z1, y2)?)?;
            let ry = sub(mul(z1, x2)?, mul(x1, z2)?)?;
            let rz = sub(mul(x1, y2)?, mul(y1, x2)?)?;

            if is_f16 {
                Some(Value::Vector(vec![
                    Value::Lit(Lit::F16(half::f16::from_f32(rx))),
                    Value::Lit(Lit::F16(half::f16::from_f32(ry))),
                    Value::Lit(Lit::F16(half::f16::from_f32(rz))),
                ]))
            } else {
                Some(Value::Vector(vec![
                    Value::Lit(Lit::F32(rx)),
                    Value::Lit(Lit::F32(ry)),
                    Value::Lit(Lit::F32(rz)),
                ]))
            }
        }
        _ => None,
    }
}

fn evaluate_face_forward(arg1: Value, arg2: Value, arg3: Value) -> Option<Value> {
    let dot = evaluate_dot(arg2, arg3)?;
    let dot_val = match dot {
        Value::Lit(Lit::F32(f)) => f,
        Value::Lit(Lit::F16(f)) => f.to_f32(),
        _ => return None,
    };
    if dot_val < 0.0 {
        Some(arg1)
    } else {
        match arg1 {
            Value::Vector(v) => {
                let mut res = Vec::new();
                for e in v {
                    match e {
                        Value::Lit(Lit::F32(f)) => res.push(Value::Lit(Lit::F32(-f))),
                        Value::Lit(Lit::F16(f)) => {
                            res.push(Value::Lit(Lit::F16(half::f16::from_f32(-f.to_f32()))))
                        }
                        _ => return None,
                    }
                }
                Some(Value::Vector(res))
            }
            Value::Lit(Lit::F32(f)) => Some(Value::Lit(Lit::F32(-f))),
            Value::Lit(Lit::F16(f)) => Some(Value::Lit(Lit::F16(half::f16::from_f32(-f.to_f32())))),
            _ => None,
        }
    }
}

fn evaluate_reflect(arg1: Value, arg2: Value) -> Option<Value> {
    let dot = evaluate_dot(arg2.clone(), arg1.clone())?;
    match (arg1, arg2, dot) {
        (Value::Vector(v1), Value::Vector(v2), Value::Lit(Lit::F32(d))) => {
            let mut res = Vec::new();
            let two_d = in_float_range(2.0 * d)?;
            for (e1, e2) in v1.into_iter().zip(v2) {
                if let (Value::Lit(Lit::F32(f1)), Value::Lit(Lit::F32(f2))) = (e1, e2) {
                    let p = in_float_range(two_d * f2)?;
                    let r = in_float_range(f1 - p)?;
                    res.push(Value::Lit(Lit::F32(r)));
                } else {
                    return None;
                }
            }
            Some(Value::Vector(res))
        }
        (Value::Vector(v1), Value::Vector(v2), Value::Lit(Lit::F16(d))) => {
            let mut res = Vec::new();
            let d32 = d.to_f32();
            let two_d = in_float16_range(half::f16::from_f32(2.0 * d32))?.to_f32();
            for (e1, e2) in v1.into_iter().zip(v2) {
                if let (Value::Lit(Lit::F16(f1)), Value::Lit(Lit::F16(f2))) = (e1, e2) {
                    let p = in_float16_range(half::f16::from_f32(two_d * f2.to_f32()))?.to_f32();
                    let r = in_float16_range(half::f16::from_f32(f1.to_f32() - p))?;
                    res.push(Value::Lit(Lit::F16(r)));
                } else {
                    return None;
                }
            }
            Some(Value::Vector(res))
        }
        (Value::Lit(Lit::F32(f1)), Value::Lit(Lit::F32(f2)), Value::Lit(Lit::F32(d))) => {
            let two_d = in_float_range(2.0 * d)?;
            let p = in_float_range(two_d * f2)?;
            Some(Value::Lit(Lit::F32(in_float_range(f1 - p)?)))
        }
        (Value::Lit(Lit::F16(f1)), Value::Lit(Lit::F16(f2)), Value::Lit(Lit::F16(d))) => {
            let d32 = d.to_f32();
            let two_d = in_float16_range(half::f16::from_f32(2.0 * d32))?.to_f32();
            let p = in_float16_range(half::f16::from_f32(two_d * f2.to_f32()))?.to_f32();
            Some(Value::Lit(Lit::F16(in_float16_range(
                half::f16::from_f32(f1.to_f32() - p),
            )?)))
        }
        _ => None,
    }
}

fn evaluate_refract(arg1: Value, arg2: Value, arg3: Value) -> Option<Value> {
    let dot = evaluate_dot(arg2.clone(), arg1.clone())?;
    let (e3, is_f16) = match arg3 {
        Value::Lit(Lit::F32(f)) => (f, false),
        Value::Lit(Lit::F16(f)) => (f.to_f32(), true),
        _ => return None,
    };
    let d = match dot {
        Value::Lit(Lit::F32(f)) => f,
        Value::Lit(Lit::F16(f)) => f.to_f32(),
        _ => return None,
    };

    let check = |f: f32| -> Option<f32> {
        if is_f16 {
            in_float16_range(half::f16::from_f32(f)).map(|x| x.to_f32())
        } else {
            in_float_range(f)
        }
    };

    let mul = |a: f32, b: f32| check(a * b);
    let add = |a: f32, b: f32| check(a + b);
    let sub = |a: f32, b: f32| check(a - b);

    let k_p1 = mul(e3, e3)?;
    let k_p2 = sub(1.0, mul(d, d)?)?;
    let k = sub(1.0, mul(k_p1, k_p2)?)?;

    if k < 0.0 {
        match arg1 {
            Value::Vector(v) => {
                let mut res = Vec::new();
                for _ in v {
                    if is_f16 {
                        res.push(Value::Lit(Lit::F16(half::f16::ZERO)));
                    } else {
                        res.push(Value::Lit(Lit::F32(0.0)));
                    }
                }
                Some(Value::Vector(res))
            }
            Value::Lit(_) => {
                if is_f16 {
                    Some(Value::Lit(Lit::F16(half::f16::ZERO)))
                } else {
                    Some(Value::Lit(Lit::F32(0.0)))
                }
            }
        }
    } else {
        let sqrt_k = check(k.sqrt())?;
        match (arg1, arg2) {
            (Value::Vector(v1), Value::Vector(v2)) => {
                let mut res = Vec::new();
                for (e1, e2) in v1.into_iter().zip(v2) {
                    let f1 = match e1 {
                        Value::Lit(Lit::F32(f)) => f,
                        Value::Lit(Lit::F16(f)) => f.to_f32(),
                        _ => return None,
                    };
                    let f2 = match e2 {
                        Value::Lit(Lit::F32(f)) => f,
                        Value::Lit(Lit::F16(f)) => f.to_f32(),
                        _ => return None,
                    };

                    let p1 = mul(e3, f1)?;
                    let p2 = add(mul(e3, d)?, sqrt_k)?;
                    let p3 = mul(p2, f2)?;
                    let r = sub(p1, p3)?;

                    if is_f16 {
                        res.push(Value::Lit(Lit::F16(half::f16::from_f32(r))));
                    } else {
                        res.push(Value::Lit(Lit::F32(r)));
                    }
                }
                Some(Value::Vector(res))
            }
            (Value::Lit(Lit::F32(f1)), Value::Lit(Lit::F32(f2))) => {
                let p1 = mul(e3, f1)?;
                let p2 = add(mul(e3, d)?, sqrt_k)?;
                let p3 = mul(p2, f2)?;
                Some(Value::Lit(Lit::F32(sub(p1, p3)?)))
            }
            (Value::Lit(Lit::F16(f1)), Value::Lit(Lit::F16(f2))) => {
                let f1 = f1.to_f32();
                let f2 = f2.to_f32();
                let p1 = mul(e3, f1)?;
                let p2 = add(mul(e3, d)?, sqrt_k)?;
                let p3 = mul(p2, f2)?;
                Some(Value::Lit(Lit::F16(half::f16::from_f32(sub(p1, p3)?))))
            }
            _ => None,
        }
    }
}

fn atan2(val1: Lit, val2: Lit) -> Option<Value> {
    match (val1, val2) {
        (Lit::F32(y), Lit::F32(x)) => Value::from_f32(in_float_range(y.atan2(x))),
        (Lit::F16(y), Lit::F16(x)) => Value::from_f16(in_float16_range(half::f16::from_f32(
            y.to_f32().atan2(x.to_f32()),
        ))),
        _ => None,
    }
}

fn evaluate_normalize(arg: Value) -> Option<Value> {
    let len_val = evaluate_length(arg.clone())?;

    let len = match len_val {
        Value::Lit(Lit::F32(f)) => f,
        Value::Lit(Lit::F16(f)) => f.to_f32(),
        _ => return None,
    };

    if len == 0.0 {
        return None;
    }

    match arg {
        Value::Vector(v) => {
            let mut res = Vec::new();
            for e in v {
                match e {
                    Value::Lit(Lit::F32(f)) => {
                        let r = in_float_range(f / len)?;
                        res.push(Value::Lit(Lit::F32(r)));
                    }
                    Value::Lit(Lit::F16(f)) => {
                        let r = in_float16_range(half::f16::from_f32(f.to_f32() / len))?;
                        res.push(Value::Lit(Lit::F16(r)));
                    }
                    _ => return None,
                }
            }
            Some(Value::Vector(res))
        }
        _ => None,
    }
}

fn evaluate_transpose(arg: Value) -> Option<Value> {
    let cols = match arg {
        Value::Vector(v) => v,
        _ => return None,
    };
    let c = cols.len();
    if c == 0 {
        return None;
    }
    let r = match &cols[0] {
        Value::Vector(v) => v.len(),
        _ => return None,
    };

    let mut res_cols = vec![Vec::with_capacity(c); r];
    for col in cols {
        if let Value::Vector(col_vec) = col {
            if col_vec.len() != r {
                return None;
            }
            for (i, val) in col_vec.into_iter().enumerate() {
                res_cols[i].push(val);
            }
        } else {
            return None;
        }
    }

    Some(Value::Vector(
        res_cols.into_iter().map(Value::Vector).collect(),
    ))
}
