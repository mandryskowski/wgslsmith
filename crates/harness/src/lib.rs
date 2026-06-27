mod dawn;
mod server;
mod wgpu;

pub mod cli;
mod daemon;

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::dawn::DawnState;
use crate::wgpu::WgpuState;
use frontend::{ExecutionError, ExecutionEvent};
use futures::executor::block_on;
use process_control::{ChildExt, Control};
use reflection::PipelineDescription;
use types::{BackendType, Config, ConfigId, Implementation};

pub struct WebGPUState {
    pub dawn_state: DawnState,
    pub wgpu_state: WgpuState,
}

impl WebGPUState {
    pub fn new() -> Self {
        Self {
            dawn_state: DawnState::new(),
            wgpu_state: WgpuState::new(),
        }
    }
}

impl Default for WebGPUState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct HarnessCommand {
    pub program: PathBuf,
    args: Vec<String>,
}

impl HarnessCommand {
    pub fn new(program: PathBuf) -> Self {
        HarnessCommand {
            program,
            args: vec![],
        }
    }
    pub fn build(&self) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.args);
        cmd
    }

    #[must_use]
    pub fn arg(&self, arg: impl Into<String>) -> HarnessCommand {
        let mut args = self.args.clone();
        args.push(arg.into());

        HarnessCommand {
            program: self.program.clone(),
            args,
        }
    }
}

pub fn query_configs() -> Vec<Config> {
    let mut configurations = vec![];

    configurations.extend(
        wgpu::get_adapters()
            .into_iter()
            .map(|adapter| Config::new(Implementation::Wgpu, adapter)),
    );

    configurations.extend(
        dawn::get_adapters()
            .into_iter()
            .map(|adapter| Config::new(Implementation::Dawn, adapter)),
    );

    configurations
}

pub fn default_configs() -> Vec<ConfigId> {
    let mut configs = vec![];
    let available = query_configs();

    let targets = [
        (Implementation::Dawn, BackendType::Dx12),
        (Implementation::Dawn, BackendType::Metal),
        (Implementation::Dawn, BackendType::Vulkan),
        (Implementation::Wgpu, BackendType::Dx12),
        (Implementation::Wgpu, BackendType::Metal),
        (Implementation::Wgpu, BackendType::Vulkan),
    ];

    for target in targets {
        if let Some(config) = available
            .iter()
            .find(|it| target == (it.id.implementation, it.id.backend))
        {
            configs.push(config.id.clone());
        }
    }

    configs
}

#[derive(bincode::Encode)]
struct ExecutionArgs<'a> {
    pub shader: &'a str,
    pub pipeline_desc: &'a PipelineDescription,
}

#[derive(bincode::Decode, bincode::Encode)]
struct ExecutionInput {
    pub shader: String,
    pub pipeline_desc: PipelineDescription,
}

#[derive(bincode::Decode, bincode::Encode)]
struct ExecutionOutput {
    pub buffers: Vec<Vec<u8>>,
}

pub fn execute<E: FnMut(ExecutionEvent) -> Result<(), ExecutionError>>(
    cmd: &HarnessCommand,
    shader: &str,
    pipeline_desc: &PipelineDescription,
    configs: &[ConfigId],
    timeout: Option<Duration>,
    mut on_event: E,
) -> Result<(), ExecutionError> {
    let default_configs;
    let configs = if configs.is_empty() {
        default_configs = crate::default_configs();

        if default_configs.is_empty() {
            return Err(ExecutionError::NoDefaultConfigs);
        }

        on_event(ExecutionEvent::UsingDefaultConfigs(default_configs.clone()))?;

        default_configs.as_slice()
    } else {
        configs
    };

    configs.iter().try_for_each(|config| {
        on_event(ExecutionEvent::Start(config.clone()))?;

        let mut child = cmd
            .build()
            .arg(config.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().unwrap();

        bincode::encode_into_std_write(
            ExecutionArgs {
                shader,
                pipeline_desc,
            },
            &mut stdin,
            bincode::config::standard(),
        )?;

        let mut child = child.controlled_with_output();
        if let Some(timeout) = timeout {
            child = child.time_limit(timeout).terminate_for_timeout();
        }

        let output = match child.wait()? {
            Some(output) => output,
            None => return on_event(ExecutionEvent::Timeout),
        };

        if output.status.success() {
            let (output, _): (ExecutionOutput, _) =
                bincode::decode_from_slice(&output.stdout, bincode::config::standard())?;
            on_event(ExecutionEvent::Success(output.buffers))
        } else {
            on_event(ExecutionEvent::Failure(output.stderr))
        }
    })
}

pub fn execute_config(
    shader: &str,
    pipeline_desc: &PipelineDescription,
    config: &ConfigId,
    state: Option<&mut WebGPUState>,
) -> eyre::Result<Vec<Vec<u8>>> {
    match config.implementation {
        Implementation::Dawn => block_on(dawn::run(
            shader,
            pipeline_desc,
            config,
            state.map(|s| &mut s.dawn_state),
        )),
        Implementation::Wgpu => block_on(wgpu::run(
            shader,
            pipeline_desc,
            config,
            state.map(|s| &mut s.wgpu_state),
        )),
    }
}
