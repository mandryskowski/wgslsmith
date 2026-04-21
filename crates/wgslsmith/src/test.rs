use std::path::PathBuf;

use ast::Module;
use clap::Parser;
use eyre::eyre;
use harness_types::ConfigId;
use regex::Regex;

use crate::compiler::{Backend, Compiler};
use crate::config::Config;
use crate::harness_runner::{ExecutionResult, Target, TargetPath};
use crate::reducer::ReductionKind;
use crate::{harness_runner, validator};

#[derive(Parser)]
pub struct Options {
    #[clap(action, action)]
    kind: ReductionKind,

    #[clap(action)]
    shader: PathBuf,

    #[clap(action)]
    input_data: Option<PathBuf>,

    #[clap(long, action)]
    server: Option<String>,

    #[clap(flatten)]
    crash_options: CrashOptions,

    #[clap(short, long, action)]
    quiet: bool,
}

#[derive(Parser)]
pub struct CrashOptions {
    #[clap(long, action, conflicts_with("compiler"))]
    config: Option<ConfigId>,

    #[clap(short = 't', long = "target", action)]
    targets: Vec<TargetPath>,

    #[clap(long, value_enum, action, requires("backend"))]
    compiler: Option<Compiler>,

    #[clap(long, value_enum, action)]
    backend: Option<Backend>,

    #[clap(long, action, required_if_eq("kind", "crash"))]
    regex: Option<Regex>,

    #[clap(long, action)]
    inverse_regex: Option<Regex>,

    #[clap(long, action)]
    no_recondition: bool,

    /// Command to run before executing the shader.
    #[clap(long, action)]
    pub pre_cmd: Option<String>,

    /// Command to run after executing the shader. If the command succeeds (exits with code 0), the shader will be considered interesting.
    #[clap(long, action)]
    pub post_cmd: Option<String>,
}

pub fn run(config: &Config, options: Options) -> eyre::Result<()> {
    if let Ok(port) = std::env::var("WGSLREDUCE_PORT") {
        if let Ok(port) = port.parse::<u16>() {
            let _ = std::net::UdpSocket::bind("127.0.0.1:0")
                .and_then(|s| s.send_to(b"1", ("127.0.0.1", port)));
        }
    }

    let source = std::fs::read_to_string(&options.shader)?;

    let input_path = if let Some(input_path) = options.input_data {
        input_path
    } else {
        let mut try_path = options
            .shader
            .parent()
            .unwrap()
            .join(options.shader.file_stem().unwrap())
            .with_extension("json");

        if !try_path.exists() {
            try_path = options.shader.parent().unwrap().join("inputs.json");
        }

        if !try_path.exists() {
            try_path = options
                .shader
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("inputs.json");
        }

        if !try_path.exists() {
            return Err(eyre!(
                "couldn't determine path to inputs file, pass one explicitly"
            ));
        }

        try_path
    };

    let metadata = std::fs::read_to_string(input_path)?;

    let configs = if let Some(c) = options.crash_options.config.clone() {
        vec![c]
    } else {
        vec![]
    };

    let targets = harness_runner::get_targets(
        config,
        &options.server,
        &configs,
        &options.crash_options.targets,
    )?;

    match options.kind {
        ReductionKind::Crash => reduce_crash(
            config,
            options.crash_options,
            source,
            metadata,
            &targets,
            options.quiet,
        )?,
        ReductionKind::Mismatch => reduce_mismatch(source, metadata, &targets, options.quiet)?,
    }

    println!("interesting :)");

    Ok(())
}

fn reduce_crash(
    config: &Config,
    options: CrashOptions,
    source: String,
    metadata: String,
    targets: &[Target],
    quiet: bool,
) -> eyre::Result<()> {
    let regex = options.regex.unwrap();
    let inverse_regex = options.inverse_regex;
    let should_recondition = !options.no_recondition;

    let source = if should_recondition {
        recondition(parser::parse(&source))
    } else {
        source
    };

    let interesting = if options.config.is_some() {
        let mut any_crash_matched = false;

        for target in targets {
            if let Some(ref pre) = options.pre_cmd {
                let _ = if cfg!(windows) {
                    std::process::Command::new("cmd").arg("/C").arg(pre).status()
                } else {
                    std::process::Command::new("sh").arg("-c").arg(pre).status()
                };
            }

            let result = harness_runner::exec_shader(target, &source, &metadata, &[], |line| {
                if !quiet {
                    println!("{line}");
                }
            })?;

            if !quiet {
                eprintln!("{result:?}");
            }

            let mut current_matches = matches!(
                result,
                ExecutionResult::Crash(ref output)
                if regex.is_match(output) && !inverse_regex.clone().map(|r| r.is_match(output)).unwrap_or(false)
            );

            if let Some(ref post) = options.post_cmd {
                let status = if cfg!(windows) {
                    std::process::Command::new("cmd").arg("/C").arg(post).status()?
                } else {
                    std::process::Command::new("sh").arg("-c").arg(post).status()?
                };

                current_matches = status.success();
            }

            if current_matches {
                any_crash_matched = true;
                break;
            }
        }
        any_crash_matched
    } else {
        let compiler = options.compiler.unwrap();
        let backend = options.backend.unwrap();
        let compiled = compiler.compile(&source, backend, false)?;

        match backend {
            Backend::Hlsl => {
                remote_validate(config, &compiled, validator::Backend::Hlsl, &regex, quiet)?
            }
            Backend::Msl => {
                remote_validate(config, &compiled, validator::Backend::Msl, &regex, quiet)?
            }
            Backend::Spirv => todo!(),
        }
    };

    if !interesting {
        return Err(eyre!("shader is not interesting"));
    }

    Ok(())
}

fn reduce_mismatch(
    source: String,
    metadata: String,
    targets: &[Target],
    quiet: bool,
) -> eyre::Result<()> {
    let module = parser::parse(&source);
    let reconditioned = recondition(module);

    Compiler::Naga.validate(&reconditioned)?;
    Compiler::Tint.validate(&reconditioned)?;

    let mut consensus: Option<Vec<u8>> = None;
    let mut mismatch_found = false;

    for target in targets {
        let result = harness_runner::exec_shader(target, &reconditioned, &metadata, &[], |line| {
            if !quiet {
                println!("{line}");
            }
        })?;

        match result {
            ExecutionResult::Mismatch(_) => {
                mismatch_found = true;
                break;
            }
            ExecutionResult::Success(e) => {
                if e.is_none() {
                    // timeout or empty result, skip for consensus
                    continue;
                }
                let e = e.unwrap();

                if let Some(ref existing_consensus) = consensus {
                    if e.output != *existing_consensus {
                        if !quiet {
                            println!("harness mismatch between targets");
                        }
                        mismatch_found = true;
                        break;
                    }
                } else {
                    consensus = Some(e.output);
                }
            }
            _ => {}
        }
    }

    if !mismatch_found {
        return Err(eyre!("shader is not interesting (no mismatch found)"));
    }

    Ok(())
}

fn recondition(module: Module) -> String {
    let reconditioned = reconditioner::recondition(module);
    let mut formatted = String::new();

    ast::writer::Writer::default()
        .write_module(&mut formatted, &reconditioned)
        .unwrap();

    formatted
}

fn remote_validate(
    config: &Config,
    source: &str,
    backend: validator::Backend,
    regex: &Regex,
    quiet: bool,
) -> eyre::Result<bool> {
    if !quiet {
        println!("[SOURCE]");
        println!("{source}");
    }

    let server = config.validator.server()?;
    let result = validator::validate(server, backend, source.to_owned())?;

    let is_interesting = match result {
        validator::ValidateResponse::Success => false,
        validator::ValidateResponse::Failure(err) => {
            if !quiet {
                println!("-----");
                println!("{err}");
            }
            regex.is_match(&err)
        }
    };

    Ok(is_interesting)
}
