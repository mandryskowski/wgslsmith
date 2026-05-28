use crate::value::Value;
use ast::{DataType, Lit, ScalarType};

// Division
pub(crate) fn is_zero(val: &Value) -> bool {
    match val {
        Value::Lit(Lit::I32(v)) => *v == 0,
        Value::Lit(Lit::U32(v)) => *v == 0,
        Value::Lit(Lit::F32(v)) => *v == 0.0,
        Value::Lit(Lit::F16(v)) => *v == half::f16::ZERO,
        Value::Vector(vec) => vec.iter().any(is_zero),
        Value::Array(vec) => vec.iter().any(is_zero),
        Value::Struct(map) => map.values().any(is_zero),
        _ => false,
    }
}

// insertBits/extractBits
pub fn is_invalid_bits_call(ident: &str, vals: &[Option<Value>]) -> bool {
    match ident {
        "extractBits" => {
            if vals.len() < 3 {
                return false;
            }
            check_offset_count_overflow(&vals[1], &vals[2])
        }
        "insertBits" => {
            if vals.len() < 4 {
                return false;
            }
            check_offset_count_overflow(&vals[2], &vals[3])
        }
        _ => false,
    }
}

fn check_offset_count_overflow(offset_arg: &Option<Value>, count_arg: &Option<Value>) -> bool {
    if let (Some(Value::Lit(Lit::U32(offset))), Some(Value::Lit(Lit::U32(count)))) =
        (offset_arg, count_arg)
    {
        (*offset as u64) + (*count as u64) > 32
    } else {
        false
    }
}

// Clamp
pub fn is_invalid_clamp_bounds(low: &Value, high: &Value) -> bool {
    match (low, high) {
        (Value::Lit(l), Value::Lit(h)) => match (l, h) {
            (Lit::I32(lv), Lit::I32(hv)) => lv > hv,
            (Lit::U32(lv), Lit::U32(hv)) => lv > hv,
            (Lit::F32(lv), Lit::F32(hv)) => lv > hv,
            (Lit::F16(lv), Lit::F16(hv)) => lv > hv,
            _ => false,
        },
        (Value::Vector(l_vec), Value::Vector(h_vec)) => {
            if l_vec.len() != h_vec.len() {
                return false;
            }
            l_vec
                .iter()
                .zip(h_vec.iter())
                .any(|(l, h)| is_invalid_clamp_bounds(l, h))
        }
        // mixed scalar and vector (broadcasting)
        (Value::Lit(_), Value::Vector(h_vec)) => {
            h_vec.iter().any(|h| is_invalid_clamp_bounds(low, h))
        }
        (Value::Vector(l_vec), Value::Lit(_)) => {
            l_vec.iter().any(|l| is_invalid_clamp_bounds(l, high))
        }
        _ => unreachable!(),
    }
}

// Smoothstep
pub fn is_invalid_smoothstep_bounds(low: &Value, high: &Value) -> bool {
    match (low, high) {
        (Value::Lit(l), Value::Lit(h)) => match (l, h) {
            (Lit::F32(lv), Lit::F32(hv)) => lv == hv,
            (Lit::F16(lv), Lit::F16(hv)) => lv == hv,
            _ => false,
        },
        (Value::Vector(l_vec), Value::Vector(h_vec)) => {
            if l_vec.len() != h_vec.len() {
                return false;
            }
            l_vec
                .iter()
                .zip(h_vec.iter())
                .any(|(l, h)| is_invalid_smoothstep_bounds(l, h))
        }
        // mixed scalar and vector (broadcasting)
        (Value::Lit(_), Value::Vector(h_vec)) => {
            h_vec.iter().any(|h| is_invalid_smoothstep_bounds(low, h))
        }
        (Value::Vector(l_vec), Value::Lit(_)) => {
            l_vec.iter().any(|l| is_invalid_smoothstep_bounds(l, high))
        }
        _ => unreachable!(),
    }
}

// ldexp
pub fn is_invalid_ldexp_call(data_type: &DataType, vals: &[Option<Value>]) -> bool {
    if vals.len() < 2 {
        return false;
    }

    let e2 = &vals[1];

    if let Some(e2_val) = e2 {
        is_invalid_ldexp_exp(data_type, e2_val)
    } else {
        false
    }
}

fn is_invalid_ldexp_exp(data_type: &DataType, e2: &Value) -> bool {
    match e2 {
        Value::Lit(Lit::I32(exp)) => {
            if let Some(ScalarType::F16) = data_type.as_scalar() {
                *exp < -15 || *exp > 15
            } else {
                *exp < -127 || *exp > 128
            }
        }
        Value::Vector(vec) => vec.iter().any(|v| is_invalid_ldexp_exp(data_type, v)),
        _ => false,
    }
}
