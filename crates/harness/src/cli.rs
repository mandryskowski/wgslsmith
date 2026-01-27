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
    },
}

pub fn run(harness_cmd: HarnessCommand, command: Command) -> eyre::Result<()> {
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
        Command::Exec { config } => internal_run(config),
        Command::Serve(options) => {
            let harness_cmd = harness_cmd.arg(if options.use_daemon {
                "daemon-exec"
            } else {
                "exec"
            });
            crate::server::run(harness_cmd, options)
        }
        Command::Daemon(options) => DaemonServer::new().main_loop(options),
        Command::DaemonExec { config } => daemon_exec(config),
    }
}

fn list() -> eyre::Result<()> {
    let frontend = frontend::Printer::new();
    frontend.print_all_configs(crate::query_configs())?;
    Ok(())
}

fn internal_run(config: ConfigId) -> eyre::Result<()> {
    let input: ExecutionInput =
        bincode::decode_from_std_read(&mut std::io::stdin(), bincode::config::standard())?;

    let output = ExecutionOutput {
        buffers: crate::execute_config(&input.shader, &input.pipeline_desc, &config, None)?,
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
            parallelism: Option<usize>,
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
