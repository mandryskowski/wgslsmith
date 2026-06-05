#[cfg(feature = "reducer")]
mod auto_reducer;
#[cfg(feature = "compiler")]
mod compiler;
mod config;
mod fmt;
mod fuzzer;
mod harness_runner;
#[cfg(feature = "reducer")]
mod reducer;
mod remote;
mod rerun_daemon;
#[cfg(feature = "reducer")]
mod test;
#[cfg(feature = "reducer")]
mod validator;

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use color_eyre::Help;
use eyre::{eyre, Context};
#[cfg(feature = "harness")]
use harness::HarnessCommand;
use harness_frontend::{read_shader_from_path, ExecutionError, ExecutionEvent};
use harness_types::ConfigId;
use reflection_types::PipelineDescription;

#[derive(Parser)]
struct Options {
    #[clap(long, action)]
    config_file: Option<PathBuf>,
    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(Parser)]
enum Cmd {
    /// Compile WGSL to an intermediate representation (HSL/SPIR-V/MSL)
    #[cfg(feature = "compiler")]
    Compile(compiler::Options),
    /// Open the wgslsmith config file in the default text editor.
    Config,
    /// Generate a random shader.
    Gen(generator::Options),
    /// Recondition a shader to add safety checks.
    Recondition(reconditioner::cli::Options),
    /// Format a shader.
    Fmt(fmt::Options),
    /// Concretize a shader.
    Concretize {
        /// Path to a wgsl shader program to concretize (use '-' for stdin).
        #[clap(action, default_value = "-")]
        input: String,

        /// Path at which to write output (use '-' for stdout).
        #[clap(short, long, action, default_value = "-")]
        output: String,
    },
    Fuzz(fuzzer::Options),
    /// Reduce a shader.
    #[cfg(feature = "reducer")]
    Reduce(reducer::Options),
    #[cfg(feature = "reducer")]
    Test(test::Options),
    #[cfg(feature = "reducer")]
    AutoReduce(auto_reducer::Options),
    /// Execute a shader.
    #[cfg(feature = "harness")]
    Run(harness_frontend::cli::RunOptions),
    #[cfg(feature = "harness")]
    Harness {
        #[clap(subcommand)]
        cmd: harness::cli::Command,
    },
    /// Interact with a remote harness server.
    Remote {
        #[clap(subcommand)]
        cmd: RemoteCmd,
        #[clap(action)]
        server: Option<String>,
    },
    /// Re-runs daemon crashes recursively.
    RerunDaemon(rerun_daemon::Options),
    /// Run Skeletal Program Enumeration (SPE).
    Spe(spe::Options),

    /// Fuse two shaders.
    Fuse {
        a: String,
        b: String,
    },
}

#[derive(Parser)]
enum RemoteCmd {
    List,
    Run(harness_frontend::cli::RunOptions),
}

fn main() -> eyre::Result<()> {
    if std::env::var("NO_COLOR") == Err(std::env::VarError::NotPresent) {
        color_eyre::install()?;
    } else {
        color_eyre::config::HookBuilder::new()
            .theme(color_eyre::config::Theme::new())
            .install()?;
    }

    let options = Options::parse();

    let config_file = options
        .config_file
        .ok_or(())
        .or_else(|_| config::default_path())
        .wrap_err("couldn't determine config file path")?;

    let config = config::Config::load(&config_file)?;

    #[cfg(feature = "harness")]
    let harness_cmd = HarnessCommand::new(std::env::current_exe().unwrap())
        .arg("harness")
        .with_errors(config.harness.errors.clone());

    match options.cmd {
        #[cfg(feature = "compiler")]
        Cmd::Compile(options) => {
            let shader = read_shader_from_path(&options.shader)?;
            println!(
                "{}",
                options
                    .compiler
                    .compile(&shader, options.backend, options.validate_output)?
            );
            Ok(())
        }
        Cmd::Config => {
            if let Some(dir) = config_file.parent() {
                fs::create_dir_all(dir)?;
            }
            edit::edit_file(&config_file)?;
            Ok(())
        }
        Cmd::Gen(options) => generator::run(options),
        Cmd::Recondition(options) => reconditioner::cli::run(options),
        Cmd::Fmt(options) => fmt::run(options),
        Cmd::Concretize { input, output } => {
            let source = read_shader_from_path(&input)?;
            let ast = parser::parse(&source);
            let concretized = concretizer::concretize(ast);

            struct Output<'a>(&'a mut dyn std::io::Write);
            impl std::fmt::Write for Output<'_> {
                fn write_str(&mut self, s: &str) -> std::fmt::Result {
                    self.0.write_all(s.as_bytes()).unwrap();
                    Ok(())
                }
            }

            let mut out: Box<dyn std::io::Write> = match output.as_str() {
                "-" => Box::new(std::io::stdout()),
                path => Box::new(fs::File::create(path)?),
            };

            ast::writer::Writer::default()
                .write_module(&mut Output(&mut out), &concretized)
                .unwrap();

            Ok(())
        }
        Cmd::Fuzz(options) => fuzzer::run(config, options),
        #[cfg(feature = "reducer")]
        Cmd::Reduce(options) => reducer::run(config, options),
        #[cfg(feature = "reducer")]
        Cmd::Test(options) => test::run(&config, options),
        #[cfg(feature = "reducer")]
        Cmd::AutoReduce(options) => auto_reducer::run(&config, options),
        #[cfg(feature = "harness")]
        Cmd::Run(options) => {
            let mut harness_cmd = harness_cmd.arg(if options.use_daemon {
                "daemon-exec"
            } else {
                "exec"
            });
            if options.use_daemon {
                if let Some(port) = options.daemon_port {
                    harness_cmd = harness_cmd.arg("--daemon-port").arg(port.to_string());
                }
            }
            harness::cli::execute(harness_cmd, options)
        }
        #[cfg(feature = "harness")]
        Cmd::Harness { cmd } => harness::cli::run(
            harness_cmd,
            cmd,
            harness::DawnFlags {
                enabled: config.dawn.enabled_flags.clone(),
                disabled: config.dawn.disabled_flags.clone(),
            },
            config.harness.backend_validation,
        ),
        Cmd::Remote { cmd, server } => {
            let address = server
                .as_deref()
                .map(|server| config.resolve_remote(server))
                .or_else(|| config.default_remote())
                .ok_or_else(|| {
                    eyre!("no remote specified and no default remote found in config")
                        .with_note(|| "specify a default remote using the `harness.remote` field in your config file")
                })?;

            match cmd {
                RemoteCmd::List => {
                    let res = remote::list(address)?;
                    harness_frontend::Printer::new().print_all_configs(res.configs)?;
                    Ok(())
                }
                RemoteCmd::Run(options) => {
                    struct Executor<'a>(&'a str);

                    impl harness_frontend::Executor for Executor<'_> {
                        fn execute(
                            &self,
                            shader: &str,
                            pipeline_desc: &PipelineDescription,
                            configs: &[ConfigId],
                            timeout: Option<Duration>,
                            _parallelism: usize,
                            compile_only: bool,
                            on_event: &mut (dyn FnMut(ExecutionEvent) -> Result<(), ExecutionError>
                                      + Send),
                        ) -> Result<(), ExecutionError> {
                            remote::execute(
                                self.0,
                                shader.to_owned(),
                                pipeline_desc.clone(),
                                configs.to_owned(),
                                timeout,
                                compile_only,
                                on_event,
                            )
                        }
                    }

                    harness_frontend::cli::run(options, &Executor(address))
                }
            }
        }
        Cmd::RerunDaemon(options) => rerun_daemon::run(&config, options),
        Cmd::Spe(options) => {
            spe::run(options);
            Ok(())
        }
        Cmd::Fuse { a, b } => {
            let shader_a = read_shader_from_path(&a)?;
            let shader_b = read_shader_from_path(&b)?;
            let module_a = parser::parse(&shader_a);
            let module_b = parser::parse(&shader_b);
            let fused_module = fuse::fuse(module_a, module_b);
            let mut out_str = String::new();
            ast::writer::Writer::default()
                .write_module(&mut out_str, &fused_module)
                .unwrap();
            println!("{}", out_str);
            Ok(())
        }
    }
}
