use clap::{Parser, ValueEnum};
use eyre::{eyre, Context};
use rspirv::binary::Disassemble;
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
pub struct Options {
    /// Path to wgsl shader program to be executed (use '-' for stdin)
    #[clap(action, default_value = "-")]
    pub shader: String,

    #[clap(long, value_enum, action, requires("backend"))]
    pub(crate) compiler: Compiler,

    #[clap(long, value_enum, action)]
    pub(crate) backend: Backend,

    #[clap(long, action)]
    pub validate_output: bool,
}

#[derive(ValueEnum, Clone)]
pub enum Compiler {
    Tint,
    Naga,
}

impl Display for Compiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = match self {
            Compiler::Tint => "tint",
            Compiler::Naga => "naga",
        };

        write!(f, "{val}")
    }
}

#[derive(ValueEnum, Clone, Copy)]
pub enum Backend {
    Hlsl,
    Msl,
    Spirv,
}

impl Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let val = match self {
            Backend::Hlsl => "hlsl",
            Backend::Msl => "msl",
            Backend::Spirv => "spirv",
        };

        write!(f, "{val}")
    }
}

impl Compiler {
    pub fn validate(&self, source: &str) -> eyre::Result<()> {
        match self {
            Compiler::Tint => validate_tint(source).wrap_err("tint validation failed"),
            Compiler::Naga => validate_naga(source).wrap_err("naga validation failed"),
        }
    }

    pub fn compile(
        &self,
        source: &str,
        backend: Backend,
        validate_output: bool,
    ) -> eyre::Result<String> {
        match self {
            Compiler::Tint => compile_tint(source, backend, validate_output),
            Compiler::Naga => compile_naga(source, backend, validate_output),
        }
    }
}

fn validate_naga(source: &str) -> eyre::Result<()> {
    use naga::front::wgsl;
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    let module = wgsl::parse_str(&source.replace("@stage(compute)", "@compute"))?;
    Validator::new(ValidationFlags::default(), Capabilities::all()).validate(&module)?;
    Ok(())
}

fn validate_tint(source: &str) -> eyre::Result<()> {
    tint::validate_shader(source).map_err(|e| eyre!("{e}"))
}

fn compile_naga(source: &str, backend: Backend, validate_output: bool) -> eyre::Result<String> {
    use naga::back::{hlsl, msl, spv};
    use naga::front::wgsl;
    use naga::valid::{Capabilities, ValidationFlags, Validator};

    let parsed_module = wgsl::parse_str(&source.replace("@stage(compute)", "@compute"))?;

    let initial_validation =
        Validator::new(ValidationFlags::default(), Capabilities::all()).validate(&parsed_module)?;

    let ep = parsed_module
        .entry_points
        .first()
        .ok_or_else(|| eyre!("no entry point found"))?;
    let ep_stage = ep.stage;
    let ep_name = ep.name.clone();

    let (module, validation) = naga::back::pipeline_constants::process_overrides(
        &parsed_module,
        &initial_validation,
        Some((ep_stage, ep_name.as_str())),
        &Default::default(),
    )
    .map_err(|e| eyre!("Failed to process overrides: {:?}", e))?;

    let mut out = String::new();

    match backend {
        Backend::Hlsl => {
            let options = hlsl::Options {
                shader_model: hlsl::ShaderModel::V5_1,
                binding_map: Default::default(),
                ..Default::default()
            };

            let pipeline_options = hlsl::PipelineOptions {
                entry_point: Some((ep_stage, ep_name)),
            };

            hlsl::Writer::new(&mut out, &options, &pipeline_options).write(
                &module,
                &validation,
                None,
            )?;

            if validate_output {
                validate_hlsl(&out)?;
            }
        }
        Backend::Msl => {
            msl::Writer::new(&mut out).write(
                &module,
                &validation,
                &msl::Options::default(),
                &msl::PipelineOptions::default(),
            )?;

            if validate_output {
                validate_msl(&out)?;
            }
        }
        Backend::Spirv => {
            let options = spv::Options::default();

            let binary = spv::write_vec(&module, &validation, &options, None)?;

            if validate_output {
                validate_spirv(&binary)?;
            }

            out = disassemble_spirv(binary)
        }
    }

    Ok(out)
}

fn compile_tint(source: &str, backend: Backend, validate_output: bool) -> eyre::Result<String> {
    let out = match backend {
        Backend::Hlsl => {
            let hlsl = tint::compile_shader_to_hlsl(source)
                .ok_or_else(|| eyre!("tint failed to compile to hlsl"))?;
            if validate_output {
                validate_hlsl(&hlsl)?;
            }
            hlsl
        }
        Backend::Msl => {
            let msl = tint::compile_shader_to_msl(source)
                .ok_or_else(|| eyre!("tint failed to compile to msl"))?;
            if validate_output {
                validate_msl(&msl)?;
            }
            msl
        }
        Backend::Spirv => {
            let binary = tint::compile_shader_to_spirv(source)
                .ok_or_else(|| eyre!("tint failed to compile to spirv"))?;
            if validate_output {
                validate_spirv(&binary)?;
            }
            disassemble_spirv(binary)
        }
    };
    Ok(out)
}

fn disassemble_spirv(binary: Vec<u32>) -> String {
    let mut loader = rspirv::dr::Loader::new();
    rspirv::binary::parse_words(&binary, &mut loader).unwrap();
    let module = loader.module();

    module.disassemble()
}

fn validate_spirv(binary: &[u32]) -> eyre::Result<()> {
    let bytes: &[u8] = bytemuck::cast_slice(binary);

    let mut child = Command::new("spirv-val")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err("Failed to spawn `spirv-val`. Is SPIRV-Tools installed in your PATH?")?;

    child.stdin.as_mut().unwrap().write_all(bytes)?;
    let output = child.wait_with_output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        return Err(eyre!(
            "Invalid SPIR-V produced:\nStdout: {}\nStderr: {}",
            out,
            err
        ));
    }
    Ok(())
}

fn validate_hlsl(source: &str) -> eyre::Result<()> {
    let pid = std::process::id();
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros();

    let temp_file_path = env::temp_dir().join(format!("wgslsmith_{}_{}.hlsl", pid, time));

    fs::write(&temp_file_path, source).wrap_err("Failed to write temporary HLSL file")?;

    let child = Command::new("dxc")
        .args(["-T", "lib_6_3", temp_file_path.to_str().unwrap()])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err("Failed to spawn `dxc`. Is it installed and in your PATH?")?;

    let output_result = child.wait_with_output();

    let _ = fs::remove_file(&temp_file_path);

    let output = output_result?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        return Err(eyre::eyre!("Invalid HLSL produced:\n{}\n{}", err, out));
    }

    Ok(())
}

fn validate_msl(source: &str) -> eyre::Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        let mut c = Command::new("xcrun");
        c.args([
            "-sdk",
            "macosx",
            "metal",
            "-x",
            "metal",
            "-c",
            "-",
            "-o",
            "/dev/null",
        ]);
        c
    } else if cfg!(target_os = "windows") {
        let mut c = Command::new("metal.exe");
        c.args(["-x", "metal", "-c", "-", "-o", "NUL"]);
        c
    } else {
        let mut c = Command::new("wine");
        c.args(["metal.exe", "-x", "metal", "-c", "-", "-o", "/dev/null"]);
        c
    };

    let mut child = cmd
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err_with(|| {
            if cfg!(target_os = "macos") {
                "Failed to spawn `xcrun metal`. Are you on macOS with Xcode installed?"
            } else if cfg!(target_os = "windows") {
                "Failed to spawn `metal.exe`. Is 'Metal Developer Tools for Windows' installed and in your PATH?"
            } else {
                "Failed to spawn `wine metal.exe`. Make sure Wine is installed and `metal.exe` is in your PATH."
            }
        })?;

    child.stdin.as_mut().unwrap().write_all(source.as_bytes())?;
    let output = child.wait_with_output()?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(eyre::eyre!("Invalid MSL produced:\n{}", err));
    }

    Ok(())
}
