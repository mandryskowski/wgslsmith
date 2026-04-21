use std::ffi::OsStr;
use std::fs::Permissions;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use std::{env, thread};

use clap::{Parser, ValueEnum};
use eyre::{eyre, Context};
use regex::Regex;
use tap::Tap;

use crate::compiler::{Backend, Compiler};
use crate::config::Config;
use crate::harness_runner::TargetPath;

#[derive(ValueEnum, Clone)]
pub enum ReductionKind {
    Crash,
    Mismatch,
}

#[derive(Parser)]
pub struct Options {
    /// Type of bug that is being reduced.
    #[clap(action, action)]
    kind: ReductionKind,

    /// Path to the WGSL shader file to reduce.
    #[clap(action)]
    shader: PathBuf,

    /// Path to the input data file.
    ///
    /// If not set, the program will look for a JSON file with the same name as the shader.
    #[clap(action)]
    input_data: Option<PathBuf>,

    /// Path to output directory for reduced shader.
    #[clap(short, long, action)]
    output: Option<PathBuf>,

    /// Address of harness server.
    #[clap(short, long, action)]
    server: Option<String>,

    /// Config to use for reducing a crash.
    ///
    /// This is only valid if we're reducing a crash.
    #[clap(long, action, conflicts_with("compiler"))]
    config: Option<String>,

    #[clap(short = 't', long = "target", action)]
    targets: Vec<TargetPath>,

    /// Compiler to use for reducing a crash.
    #[clap(long, action, action, requires("backend"))]
    compiler: Option<Compiler>,

    /// Compiler backend to use for reducing a crash.
    #[clap(long, action, action)]
    backend: Option<Backend>,

    /// Regex to match crash output against.
    ///
    /// This is only valid if we're reducing a crash.
    #[clap(long, action, required_if_eq("kind", "crash"))]
    regex: Option<Regex>,

    /// Inverse regex to match crash output against.
    ///
    /// This is only valid if we're reducing a crash.
    #[clap(long, action)]
    inverse_regex: Option<Regex>,

    /// Don't recondition shader before executing.
    ///
    /// This is only valid if we're reducing a crash.
    #[clap(long, action)]
    no_recondition: bool,

    /// Disable logging from harness.
    #[clap(short, long, action)]
    quiet: bool,

    #[clap(long, action, action)]
    reducer: Option<Reducer>,

    /// This passed to the underlying reducer using the appropriate flag, to set how many threads it
    /// should use.
    ///
    /// Can also be set in `wgslsmith.toml`, as `reducer.parallelism`.
    #[clap(long, action)]
    parallelism: Option<u32>,

    /// Command to run before executing the shader.
    ///
    /// This is only valid if we're reducing a crash.
    #[clap(long, action)]
    pre_cmd: Option<String>,

    /// Command to run after executing the shader. If the command succeeds (exits with code 0), the shader will be considered interesting.
    ///
    /// This is only valid if we're reducing a crash.
    #[clap(long, action)]
    post_cmd: Option<String>,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum Reducer {
    Creduce,
    Cvise,
    Perses,
    Picire,
}

impl Reducer {
    fn cmd(
        &self,
        config: &Config,
        threads: u32,
        shader: impl AsRef<OsStr>,
        test: impl AsRef<OsStr>,
    ) -> eyre::Result<Command> {
        fn build_creduce(
            path: &str,
            shader: impl AsRef<OsStr>,
            test: impl AsRef<OsStr>,
            threads: u32,
        ) -> Command {
            let mut cmd = Command::new(path);
            cmd.arg(test);
            cmd.arg(shader);
            cmd.arg("--not-c");
            cmd.arg("--n").arg(threads.to_string());
            cmd
        }

        match self {
            Reducer::Creduce => Ok(build_creduce(
                config.reducer.creduce.path(),
                shader,
                test,
                threads,
            )),
            Reducer::Cvise => Ok(build_creduce(
                config.reducer.cvise.path(),
                shader,
                test,
                threads,
            )),
            Reducer::Perses => {
                let perses_jar = config.reducer.perses.jar()?;

                Ok(Command::new("java").tap_mut(|cmd| {
                    cmd.args(["-jar", perses_jar])
                        .arg("-i")
                        .arg(shader)
                        .arg("-t")
                        .arg(test)
                        .arg("-o")
                        .arg(".")
                        .arg("--threads")
                        .arg(threads.to_string());
                }))
            }
            Reducer::Picire => Ok(Command::new("picire").tap_mut(|cmd| {
                cmd.arg("-i")
                    .arg(shader)
                    .arg("--test")
                    .arg(test)
                    .arg("--parallel")
                    .args(["-o", "."])
                    .arg("-j")
                    .arg(threads.to_string());
            })),
        }
    }

    fn gen_test_script(&self) -> String {
        let exe = env::current_exe().unwrap();
        if cfg!(windows) {
            let template = match self {
                Reducer::Picire => {
                    r#"@echo off
setlocal EnableDelayedExpansion
set args=%WGSLREDUCE_KIND% %1 %WGSLREDUCE_METADATA_PATH%
if defined WGSLREDUCE_SERVER ( set args=!args! --server %WGSLREDUCE_SERVER% )
if "%WGSLREDUCE_KIND%"=="crash" (
    set args=!args! --regex "%WGSLREDUCE_REGEX%"
    if defined WGSLREDUCE_CONFIG (
        set args=!args! --config %WGSLREDUCE_CONFIG%
    ) else (
        set args=!args! --compiler %WGSLREDUCE_COMPILER% --backend %WGSLREDUCE_BACKEND%
    )
    if not defined WGSLREDUCE_RECONDITION ( set args=!args! --no-recondition )
)
"[WGSLSMITH]" test -q !args! >nul 2>&1
"#
                }
                _ => {
                    r#"@echo off
setlocal EnableDelayedExpansion
set args=%WGSLREDUCE_KIND% %WGSLREDUCE_SHADER_NAME% %WGSLREDUCE_METADATA_PATH%
if defined WGSLREDUCE_SERVER ( set args=!args! --server %WGSLREDUCE_SERVER% )
if defined WGSLREDUCE_TARGETS ( set args=!args! %WGSLREDUCE_TARGETS% )
if "%WGSLREDUCE_KIND%"=="crash" (
    set args=!args! --regex "%WGSLREDUCE_REGEX%"
    if defined WGSLREDUCE_INVERSE_REGEX ( set args=!args! --inverse-regex "%WGSLREDUCE_INVERSE_REGEX%" )
    if defined WGSLREDUCE_CONFIG (
        set args=!args! --config %WGSLREDUCE_CONFIG%
    ) else (
        set args=!args! --compiler %WGSLREDUCE_COMPILER% --backend %WGSLREDUCE_BACKEND%
    )
    if not defined WGSLREDUCE_RECONDITION ( set args=!args! --no-recondition )
    if defined WGSLREDUCE_PRE_CMD ( set args=!args! --pre-cmd "%WGSLREDUCE_PRE_CMD%" )
    if defined WGSLREDUCE_POST_CMD ( set args=!args! --post-cmd "%WGSLREDUCE_POST_CMD%" )
)
"[WGSLSMITH]" test -q !args!
"#
                }
            };
            template.replacen("[WGSLSMITH]", exe.to_str().unwrap(), 1)
        } else {
            let template = match self {
                Reducer::Picire => include_str!("test-picire.sh"),
                _ => include_str!("test.sh"),
            };
            template.replacen("[WGSLSMITH]", exe.to_str().unwrap(), 1)
        }
    }
}

pub fn run(config: Config, options: Options) -> eyre::Result<()> {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    std::env::set_var("WGSLREDUCE_PORT", port.to_string());

    let (tx, rx) = crossbeam_channel::bounded(1);
    let worker = thread::spawn(move || {
        let result = thread_main(&config, options);
        let _ = tx.send(result);
    });

    let mut count = 0;
    let mut buf = [0; 1];

    socket.set_read_timeout(Some(std::time::Duration::from_millis(100)))?;

    loop {
        match socket.recv_from(&mut buf) {
            Ok(_) => count += 1,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::WouldBlock && e.kind() != std::io::ErrorKind::TimedOut {
                    // Ignore other errors
                }
            }
        }

        if let Ok(result) = rx.try_recv() {
            result?;
            break;
        }
    }

    if worker.join().is_err() {
        return Err(eyre!("Worker thread panicked"));
    }

    println!("> {count} calls to interestingness test");

    Ok(())
}

fn thread_main(config: &Config, options: Options) -> eyre::Result<()> {
    let shader_path = Path::new(&options.shader);
    if !shader_path.exists() {
        return Err(eyre!("shader at {shader_path:?} does not exist"));
    }

    let shader_path = shader_path.canonicalize()?;

    let input_path = if let Some(input_path) = options.input_data {
        input_path
    } else {
        let mut try_path = shader_path
            .parent()
            .unwrap()
            .join(shader_path.file_stem().unwrap())
            .with_extension("json");

        if !try_path.exists() {
            try_path = shader_path.parent().unwrap().join("inputs.json");
        }

        if !try_path.exists() {
            return Err(eyre!(
                "couldn't determine path to inputs file, pass one explicitly"
            ));
        }

        try_path
    };

    if !input_path.exists() {
        return Err(eyre!("file at {input_path:?} does not exist"));
    }

    let metadata_path = input_path.canonicalize()?;

    let out_dir = options.output.unwrap_or_else(|| {
        let out_dir = options.shader.parent().unwrap().join("reduced");
        if out_dir.exists() {
            let mut n = 1;
            loop {
                let path = out_dir.with_file_name(format!("reduced-{n}"));
                if !path.exists() {
                    break path;
                }
                n += 1
            }
        } else {
            out_dir
        }
    });

    let shader_name = options.shader.file_name().unwrap();

    let reducer = options.reducer.unwrap_or_else(|| {
        if config.reducer.perses.jar.is_some() {
            Reducer::Perses
        } else {
            Reducer::Creduce
        }
    });

    println!("> using reducer: {reducer:?}");

    setup_out_dir(&out_dir, &options.shader, &reducer)?;

    let harness_server = options
        .server
        .as_deref()
        .or_else(|| config.default_remote());

    let parallelism = options
        .parallelism
        .or(config.reducer.parallelism)
        .unwrap_or(1);

    let test_name = if cfg!(windows) { "test.bat" } else { "test.sh" };
    let mut cmd = reducer
        .cmd(config, parallelism, shader_name, test_name)?
        .tap_mut(|cmd| {
            cmd.current_dir(&out_dir)
                .env("WGSLREDUCE_SHADER_NAME", shader_path.file_name().unwrap())
                .env("WGSLREDUCE_METADATA_PATH", metadata_path);

            if let Some(server) = harness_server {
                cmd.env("WGSLREDUCE_SERVER", server);
            }

            if let Some(tmpdir) = &config.reducer.tmpdir {
                cmd.env("TMPDIR", tmpdir);
            }

            let targets = options
                .targets
                .iter()
                .map(|t| "--target ".to_owned() + &t.to_string())
                .collect::<Vec<_>>()
                .join(" ");

            if !targets.is_empty() {
                cmd.env("WGSLREDUCE_TARGETS", targets);
            }
        });

    match options.kind {
        ReductionKind::Crash => {
            cmd.env("WGSLREDUCE_KIND", "crash")
                .env("WGSLREDUCE_REGEX", options.regex.unwrap().as_str());

            if let Some(inverse_regex) = options.inverse_regex {
                cmd.env("WGSLREDUCE_INVERSE_REGEX", inverse_regex.as_str());
            }

            if let Some(config) = options.config {
                cmd.env("WGSLREDUCE_CONFIG", config);
            } else {
                let compiler = options.compiler.unwrap();
                let backend = options.backend.unwrap();
                cmd.env("WGSLREDUCE_COMPILER", compiler.to_string())
                    .env("WGSLREDUCE_BACKEND", backend.to_string());
            }

            if !options.no_recondition {
                cmd.env("WGSLREDUCE_RECONDITION", "1");
            }

            if let Some(pre) = &options.pre_cmd {
                cmd.env("WGSLREDUCE_PRE_CMD", pre);
            }

            if let Some(post) = &options.post_cmd {
                cmd.env("WGSLREDUCE_POST_CMD", post);
            }
        }
        ReductionKind::Mismatch => {
            cmd.env("WGSLREDUCE_KIND", "mismatch");
        }
    }

    let start_time = Instant::now();

    if !cmd.status()?.success() {
        return Err(eyre!("reducer process did not exit successfully"));
    }

    let end_time = Instant::now();
    let duration = end_time - start_time;

    println!("> reducer completed in {}s", duration.as_secs_f64());

    let result_path = out_dir.join(shader_name).to_str().unwrap().to_owned();
    // let reconditioned_path = out_dir
    //     .join("reconditioned.wgsl")
    //     .to_str()
    //     .unwrap()
    //     .to_owned();

    crate::fmt::run(crate::fmt::Options {
        input: result_path.clone(),
        output: result_path,
    })?;

    // crate::reconditioner::run(crate::reconditioner::Options {
    //     input: result_path,
    //     output: reconditioned_path,
    // })?;

    Ok(())
}

fn setup_out_dir(out_dir: &Path, shader: &Path, reducer: &Reducer) -> eyre::Result<()> {
    // Create output dir
    if !out_dir.exists() {
        std::fs::create_dir(out_dir)
            .wrap_err_with(|| eyre!("failed to create dir `{}`", out_dir.display()))?;
    } else if std::fs::read_dir(out_dir)?.next().is_some() {
        return Err(eyre!("`{}` is not empty", out_dir.display()));
    }

    // Copy over the shader file
    std::fs::copy(shader, out_dir.join(shader.file_name().unwrap()))?;

    // Generate the interestingness test script
    let test_path = out_dir.join(if cfg!(windows) { "test.bat" } else { "test.sh" });
    std::fs::write(&test_path, reducer.gen_test_script())?;

    #[cfg(target_family = "unix")]
    {
        use std::os::unix::fs::PermissionsExt;
        // Make sure the test script is executable
        std::fs::set_permissions(test_path, std::fs::Permissions::from_mode(0o755))?;
    }

    Ok(())
}
