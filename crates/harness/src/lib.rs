mod dawn;
mod server;
mod wgpu;

pub mod cli;

use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

use frontend::{ExecutionError, ExecutionEvent};
use futures::executor::block_on;
use process_control::{ChildExt, Control};
use reflection::PipelineDescription;
use types::{BackendType, Config, ConfigId, Implementation};

pub trait HarnessHost {
    fn exec_command() -> Command;
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

#[derive(bincode::Decode)]
struct ExecutionInput {
    pub shader: String,
    pub pipeline_desc: PipelineDescription,
}

#[derive(bincode::Decode, bincode::Encode)]
struct ExecutionOutput {
    pub buffers: Vec<Vec<u8>>,
}

pub fn execute<Host: HarnessHost, E: FnMut(ExecutionEvent) -> Result<(), ExecutionError> + Send>(
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

    let on_event_mutex = Mutex::new(on_event);

    std::thread::scope(|s| {
        let mut handles = vec![];

        for config in configs {
            let on_event = &on_event_mutex;

            handles.push(s.spawn(move || -> Result<(), ExecutionError> {
                {
                    let mut lock = on_event.lock().expect("mutex poisoned");
                    lock(ExecutionEvent::Start(config.clone()))?;
                }

                let mut child = Host::exec_command()
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

                let output = child.wait()?;

                let mut lock = on_event.lock().expect("mutex poisoned");

                match output {
                    Some(output) => {
                        if output.status.success() {
                            let (output, _): (ExecutionOutput, _) =
                                bincode::decode_from_slice(&output.stdout, bincode::config::standard())?;
                            lock(ExecutionEvent::Success(output.buffers))
                        } else {
                            lock(ExecutionEvent::Failure(output.stderr))
                        }
                    }
                    None => lock(ExecutionEvent::Timeout),
                }
            }));
        }

        for handle in handles {
            if let Err(e) = handle.join().unwrap() {
                return Err(e);
            }
        }

        Ok(())
    })
}

pub fn execute_config(
    shader: &str,
    pipeline_desc: &PipelineDescription,
    config: &ConfigId,
) -> eyre::Result<Vec<Vec<u8>>> {
    match config.implementation {
        Implementation::Dawn => block_on(dawn::run(shader, pipeline_desc, config)),
        Implementation::Wgpu => block_on(wgpu::run(shader, pipeline_desc, config)),
    }
}
