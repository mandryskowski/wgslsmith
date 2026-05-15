use clap::Parser;
use frontend::cli::RunOptions;
use frontend::ExecutionError;
use reflection::PipelineDescription;
use std::time::Duration;
use types::ConfigId;

use crate::daemon::{daemon_exec, DaemonOptions, DaemonServer};
use crate::{ExecutionEvent, ExecutionInput, ExecutionOutput, HarnessCommand};

#[derive(Parser)]
pub enum Command {
    /// Lists available configurations that can be used to execute a shader.
    List,

    /// Runs a wgsl shader against one or more configurations.
    Run(RunOptions),

    #[clap(hide(true))]
    Exec {
        #[clap(action)]
        config: ConfigId,
    },

    /// Runs the harness server for remote execution.
    Serve(crate::server::Options),

    /// Runs as a daemon which persists dawn and wgpu state.
    /// Initialising WebGPU implementations takes a long time (~5s). We do not want to do this
    /// for every shader execution.
    Daemon(DaemonOptions),

    DaemonExec {
        #[clap(action)]
        config: ConfigId,
        #[clap(long, action)]
        daemon_port: Option<u16>,
    },
}

pub fn run(
    harness_cmd: HarnessCommand,
    command: Command,
    dawn_flags: crate::DawnFlags,
) -> eyre::Result<()> {
    match command {
        Command::List => list(),
        Command::Run(options) => {
            let harness_cmd = harness_cmd.arg(if options.use_daemon {
                "daemon-exec"
            } else {
                "exec"
            });
            execute(harness_cmd, options)
        }
        Command::Exec { config } => internal_run(config, dawn_flags),
        Command::Serve(options) => {
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
            crate::server::run(harness_cmd, options)
        }
        Command::Daemon(options) => DaemonServer::new(dawn_flags).main_loop(options),
        Command::DaemonExec {
            config,
            daemon_port,
        } => daemon_exec(config, daemon_port),
    }
}

fn list() -> eyre::Result<()> {
    let frontend = frontend::Printer::new();
    frontend.print_all_configs(crate::query_configs())?;
    Ok(())
}

fn internal_run(config: ConfigId, dawn_flags: crate::DawnFlags) -> eyre::Result<()> {
    let input: ExecutionInput =
        bincode::decode_from_std_read(&mut std::io::stdin(), bincode::config::standard())?;

    let mut state = crate::WebGPUState::new(dawn_flags);
    let output = ExecutionOutput {
        buffers: crate::execute_config(
            &input.shader,
            &input.pipeline_desc,
            &config,
            Some(&mut state),
        )?,
        stderr: String::new(),
    };

    bincode::encode_into_std_write(output, &mut std::io::stdout(), bincode::config::standard())?;

    Ok(())
}

pub fn execute(cmd: HarnessCommand, options: RunOptions) -> eyre::Result<()> {
    struct Executor {
        cmd: HarnessCommand,
    }

    impl frontend::Executor for Executor {
        fn execute(
            &self,
            shader: &str,
            pipeline_desc: &PipelineDescription,
            configs: &[ConfigId],
            timeout: Option<Duration>,
            parallelism: usize,
            on_event: &mut (dyn FnMut(ExecutionEvent) -> Result<(), ExecutionError> + Send),
        ) -> Result<(), ExecutionError> {
            crate::execute::<_>(
                &self.cmd,
                shader,
                pipeline_desc,
                configs,
                timeout,
                parallelism,
                on_event,
            )
        }
    }

    frontend::cli::run(options, &Executor { cmd })
}
