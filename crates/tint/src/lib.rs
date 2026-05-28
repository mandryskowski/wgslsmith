use std::ffi::CString;

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("tint/src/lib.h");
        unsafe fn validate_shader(source: *const c_char) -> bool;
        unsafe fn compile_shader_to_hlsl(source: *const c_char) -> UniquePtr<CxxString>;
        unsafe fn compile_shader_to_msl(source: *const c_char) -> UniquePtr<CxxString>;
        unsafe fn compile_shader_to_spirv(source: *const c_char) -> UniquePtr<CxxVector<u32>>;
    }
}

pub fn validate_shader(source: &str) -> bool {
    let source = CString::new(source).unwrap();
    unsafe { ffi::validate_shader(source.as_ptr()) }
}

pub fn compile_shader_to_hlsl(source: &str) -> Option<String> {
    let source = CString::new(source).unwrap();
    unsafe { ffi::compile_shader_to_hlsl(source.as_ptr()) }
        .as_ref()
        .map(ToString::to_string)
}

pub fn compile_shader_to_msl(source: &str) -> Option<String> {
    let source = CString::new(source).unwrap();
    unsafe { ffi::compile_shader_to_msl(source.as_ptr()) }
        .as_ref()
        .map(ToString::to_string)
}

pub fn compile_shader_to_spirv(source: &str) -> Option<Vec<u32>> {
    let source = CString::new(source).unwrap();
    unsafe { ffi::compile_shader_to_spirv(source.as_ptr()) }
        .as_ref()
        .map(|v| v.as_slice().to_vec())
}
