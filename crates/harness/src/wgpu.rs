use crate::ConfigId;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use reflection::{PipelineDescription, ResourceKind};
use std::borrow::Cow;
use std::collections::HashMap;
use wgpu::wgt::PollType::Wait;
use wgpu::{
    Backends, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Device,
    DeviceDescriptor, ErrorFilter, ErrorScopeGuard, Instance, Limits, MapMode, Queue,
    ShaderModuleDescriptor, ShaderSource,
};

pub struct WgpuState {
    instance: Instance,
    device_cache: HashMap<ConfigId, (Device, Queue)>,
}

impl WgpuState {
    pub(crate) fn new() -> Self {
        WgpuState {
            instance: Instance::new(Self::instance_descriptor()),
            device_cache: HashMap::new(),
        }
    }

    fn instance_descriptor() -> wgpu::InstanceDescriptor {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = Backends::all();
        desc
    }
}

pub fn get_adapters() -> Vec<types::Adapter> {
    let wgpu_state = WgpuState::new();
    let instance = wgpu_state.instance;

    let adapters = futures::executor::block_on(instance.enumerate_adapters(Backends::all()));
    adapters
        .into_iter()
        .filter_map(|adapter| {
            let info = adapter.get_info();
            Some(types::Adapter {
                name: info.name,
                device_id: info.device,
                backend: match info.backend {
                    wgpu::Backend::Vulkan => crate::BackendType::Vulkan,
                    wgpu::Backend::Metal => crate::BackendType::Metal,
                    wgpu::Backend::Dx12 => crate::BackendType::Dx12,
                    wgpu::Backend::Gl => return None,
                    wgpu::Backend::BrowserWebGpu => return None,
                    _ => return None,
                },
            })
        })
        .collect()
}

pub async fn run(
    shader: &str,
    meta: &PipelineDescription,
    config: &ConfigId,
    wgpu_state: Option<&mut WgpuState>,
) -> Result<Vec<Vec<u8>>> {
    let backend = match config.backend {
        crate::BackendType::Dx12 => wgpu::Backend::Dx12,
        crate::BackendType::Metal => wgpu::Backend::Metal,
        crate::BackendType::Vulkan => wgpu::Backend::Vulkan,
    };

    let mut _owned_wgpu_state;
    let wgpu_state: &mut WgpuState = match wgpu_state {
        Some(state) => state,
        None => {
            _owned_wgpu_state = WgpuState::new();
            &mut _owned_wgpu_state
        }
    };

    // move to WgpuState!
    let (device, queue) = {
        if let Some((d, q)) = wgpu_state.device_cache.get(config) {
            (d.clone(), q.clone())
        } else {
            let instance = &wgpu_state.instance;
            let adapters = instance.enumerate_adapters(Backends::all()).await;
            let adapter = adapters
                .into_iter()
                .find(|adapter| {
                    let info = adapter.get_info();
                    info.device == config.device_id && info.backend == backend
                })
                .ok_or_else(|| eyre!("no adapter found matching id: {config}"))?;

            let required_features =
                meta.extensions
                    .iter()
                    .fold(wgpu::Features::empty(), |acc, enable| {
                        acc | match enable {
                            reflection::Extension::F16 => wgpu::Features::SHADER_F16,
                            reflection::Extension::Subgroups => wgpu::Features::SUBGROUP,
                        }
                    });

            let device_descriptor = DeviceDescriptor {
                required_limits: Limits {
                    // This is needed to support swiftshader
                    max_storage_textures_per_shader_stage: 4,
                    ..Default::default()
                },
                required_features,
                ..Default::default()
            };

            let (device, queue) = adapter.request_device(&device_descriptor).await?;

            wgpu_state
                .device_cache
                .insert(config.clone(), (device.clone(), queue.clone()));

            (device, queue)
        }
    };

    let preprocessor_opts = preprocessor::Options {
        module_scope_constants: false,
    };

    let preprocessed = preprocessor::preprocess(preprocessor_opts, shader.to_owned());

    let shader_module = ErrorScope::new(&device, vec![ErrorFilter::Validation])
        .execute(|| {
            device.create_shader_module(ShaderModuleDescriptor {
                label: None,
                source: ShaderSource::Wgsl(Cow::Owned(preprocessed)),
            })
        })
        .await?;

    let pipeline = ErrorScope::new(
        &device,
        vec![ErrorFilter::Internal, ErrorFilter::Validation],
    )
    .execute(|| {
        device.create_compute_pipeline(&ComputePipelineDescriptor {
            entry_point: Some("main"),
            label: None,
            module: &shader_module,
            layout: None,
            cache: None,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        })
    })
    .await?;

    let mut resource_buffers = vec![];

    enum ResourceBuffer {
        Storage {
            group: u32,
            binding: u32,
            size: u64,
            gpu_buffer: Buffer,
            staging_buffer: Buffer,
        },
        Uniform {
            group: u32,
            binding: u32,
            buffer: Buffer,
        },
    }

    for resource in &meta.resources {
        let size = resource.size as u64;
        match resource.kind {
            ResourceKind::StorageBuffer => {
                let gpu_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Storage GPU Buffer"),
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                    size,
                    mapped_at_creation: resource.init.is_some(),
                });

                if let Some(init) = resource.init.as_deref() {
                    gpu_buffer
                        .slice(..)
                        .get_mapped_range_mut()
                        .copy_from_slice(init);

                    gpu_buffer.unmap();
                }

                let staging_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Storage Staging Buffer"),
                    usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                    size,
                    mapped_at_creation: false,
                });

                resource_buffers.push(ResourceBuffer::Storage {
                    group: resource.group,
                    binding: resource.binding,
                    size,
                    gpu_buffer,
                    staging_buffer,
                });
            }
            ResourceKind::UniformBuffer => {
                let buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Uniform Buffer"),
                    usage: BufferUsages::UNIFORM,
                    size,
                    mapped_at_creation: true,
                });

                if let Some(init) = resource.init.as_deref() {
                    buffer
                        .slice(..)
                        .get_mapped_range_mut()
                        .copy_from_slice(init);
                }

                buffer.unmap();

                resource_buffers.push(ResourceBuffer::Uniform {
                    group: resource.group,
                    binding: resource.binding,
                    buffer,
                });
            }
        }
    }

    let mut bind_groups = HashMap::new();
    let mut groups = std::collections::HashSet::new();

    for res in &resource_buffers {
        let (group, binding, resource) = match res {
            ResourceBuffer::Storage {
                group,
                binding,
                gpu_buffer,
                ..
            } => (*group, *binding, gpu_buffer.as_entire_binding()),
            ResourceBuffer::Uniform {
                group,
                binding,
                buffer,
            } => (*group, *binding, buffer.as_entire_binding()),
        };

        groups.insert(group);
        bind_groups
            .entry(group)
            .or_insert_with(Vec::new)
            .push(BindGroupEntry { binding, resource });
    }

    let mut final_bind_groups = HashMap::new();

    for group in groups {
        let bind_group_layout = ErrorScope::new(&device, vec![ErrorFilter::Validation])
            .execute(|| pipeline.get_bind_group_layout(group))
            .await?;

        let entries = bind_groups.get(&group).unwrap();

        let bind_group = ErrorScope::new(&device, vec![ErrorFilter::Validation])
            .execute(|| {
                device.create_bind_group(&BindGroupDescriptor {
                    layout: &bind_group_layout,
                    label: None,
                    entries,
                })
            })
            .await?;

        final_bind_groups.insert(group, bind_group);
    }

    let commands = {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&pipeline);
            for (group, bind_group) in &final_bind_groups {
                pass.set_bind_group(*group, bind_group, &[]);
            }
            pass.dispatch_workgroups(1, 1, 1);
        }

        for res in &resource_buffers {
            if let ResourceBuffer::Storage {
                size,
                gpu_buffer,
                staging_buffer,
                ..
            } = res
            {
                encoder.copy_buffer_to_buffer(gpu_buffer, 0, staging_buffer, 0, *size);
            }
        }

        ErrorScope::new(
            &device,
            vec![ErrorFilter::Internal, ErrorFilter::Validation],
        )
        .execute(|| encoder.finish())
        .await?
    };

    let submission_index = queue.submit(std::iter::once(commands));

    let mut pending_mappings = vec![];

    for res in &resource_buffers {
        if let ResourceBuffer::Storage { staging_buffer, .. } = res {
            let slice = staging_buffer.slice(..);
            let (tx, rx) = futures::channel::oneshot::channel();

            slice.map_async(MapMode::Read, move |res| {
                // ignore send errors if receiver dropped
                let _ = tx.send(res);
            });

            pending_mappings.push((rx, slice, staging_buffer));
        }
    }

    device.poll(Wait {
        submission_index: Some(submission_index),
        timeout: None,
    })?;

    let mut results = vec![];

    for (rx, slice, raw_buffer) in pending_mappings {
        let map_result = rx.await?;
        map_result?; // propagate mapping errors

        let bytes = slice.get_mapped_range();
        results.push(bytes.to_vec());

        drop(bytes);
        raw_buffer.unmap();
    }

    Ok(results)
}

pub struct ErrorScope<'a> {
    device: &'a Device,
    filters: Vec<ErrorFilter>,
}

impl<'a> ErrorScope<'a> {
    pub fn new(device: &'a Device, filters: Vec<ErrorFilter>) -> Self {
        Self { device, filters }
    }

    pub async fn execute<F, T>(self, func: F) -> Result<T>
    where
        F: FnOnce() -> T,
    {
        let scopes: Vec<ErrorScopeGuard> = self
            .filters
            .into_iter()
            .map(|filter| self.device.push_error_scope(filter))
            .collect();
        let result = func();

        // we capture the first error we see, but we must pop all scopes.
        let mut caught_error = None;
        for scope in scopes.into_iter().rev() {
            if let Some(error) = scope.pop().await {
                if caught_error.is_none() {
                    caught_error = Some(error);
                }
            }
        }

        if let Some(error) = caught_error {
            return Err(eyre!("{}", error));
        }
        Ok(result)
    }
}
