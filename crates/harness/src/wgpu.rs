use crate::ConfigId;
use color_eyre::eyre::eyre;
use color_eyre::Result;
use reflection::{PipelineDescription, ResourceKind};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use wgpu::wgt::PollType::Wait;
use wgpu::{
    Backends, BindGroupDescriptor, BindGroupEntry, Buffer, BufferDescriptor, BufferUsages,
    CommandEncoderDescriptor, ComputePassDescriptor, ComputePipelineDescriptor, Device,
    DeviceDescriptor, Dx12BackendOptions, Dx12Compiler, ErrorFilter, ErrorScopeGuard, Instance,
    Limits, MapMode, Queue, ShaderModuleDescriptor, ShaderSource,
};

const CANARY_SIZE: u64 = 256;
const CANARY_VAL: u8 = 0xCD;

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
        wgpu::InstanceDescriptor {
            backends: Backends::all(),
            backend_options: wgpu::BackendOptions {
                gl: Default::default(),
                dx12: Self::dx12_backend_options(),
                noop: Default::default(),
            },
            display: Default::default(),
            flags: Default::default(),
            memory_budget_thresholds: Default::default(),
        }
    }

    fn dx12_backend_options() -> Dx12BackendOptions {
        let dxc_path = Self::dx12_get_dxc_path();

        if let Some(dxc_path) = dxc_path {
            Dx12BackendOptions {
                shader_compiler: Dx12Compiler::DynamicDxc {
                    dxc_path: dxc_path.to_string_lossy().into_owned(),
                },
                ..Default::default()
            }
        } else {
            Dx12BackendOptions::default()
        }
    }

    fn dx12_get_dxc_path() -> Option<PathBuf> {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(parent_dir) = exe_path.parent() {
                let dxc_path = parent_dir.join("dxcompiler.dll");
                if dxc_path.exists() {
                    return Some(dxc_path);
                }
            }
        }
        None
    }
}

fn map_texture_format_wgpu(format: Option<reflection::TextureFormat>) -> wgpu::TextureFormat {
    use reflection::TextureFormat::*;
    match format {
        Some(Rgba8Unorm) => wgpu::TextureFormat::Rgba8Unorm,
        Some(Rgba8Snorm) => wgpu::TextureFormat::Rgba8Snorm,
        Some(Rgba8Uint) => wgpu::TextureFormat::Rgba8Uint,
        Some(Rgba8Sint) => wgpu::TextureFormat::Rgba8Sint,
        Some(Rgba16Unorm) => wgpu::TextureFormat::Rgba16Unorm,
        Some(Rgba16Snorm) => wgpu::TextureFormat::Rgba16Snorm,
        Some(Rgba16Uint) => wgpu::TextureFormat::Rgba16Uint,
        Some(Rgba16Sint) => wgpu::TextureFormat::Rgba16Sint,
        Some(Rgba16Float) => wgpu::TextureFormat::Rgba16Float,
        Some(Rg8Unorm) => wgpu::TextureFormat::Rg8Unorm,
        Some(Rg8Snorm) => wgpu::TextureFormat::Rg8Snorm,
        Some(Rg8Uint) => wgpu::TextureFormat::Rg8Uint,
        Some(Rg8Sint) => wgpu::TextureFormat::Rg8Sint,
        Some(Rg16Unorm) => wgpu::TextureFormat::Rg16Unorm,
        Some(Rg16Snorm) => wgpu::TextureFormat::Rg16Snorm,
        Some(Rg16Uint) => wgpu::TextureFormat::Rg16Uint,
        Some(Rg16Sint) => wgpu::TextureFormat::Rg16Sint,
        Some(Rg16Float) => wgpu::TextureFormat::Rg16Float,
        Some(R32Uint) => wgpu::TextureFormat::R32Uint,
        Some(R32Sint) => wgpu::TextureFormat::R32Sint,
        Some(R32Float) => wgpu::TextureFormat::R32Float,
        Some(Rg32Uint) => wgpu::TextureFormat::Rg32Uint,
        Some(Rg32Sint) => wgpu::TextureFormat::Rg32Sint,
        Some(Rg32Float) => wgpu::TextureFormat::Rg32Float,
        Some(Rgba32Uint) => wgpu::TextureFormat::Rgba32Uint,
        Some(Rgba32Sint) => wgpu::TextureFormat::Rgba32Sint,
        Some(Rgba32Float) => wgpu::TextureFormat::Rgba32Float,
        Some(Bgra8Unorm) => wgpu::TextureFormat::Bgra8Unorm,
        Some(Bgra8UnormSrgb) => wgpu::TextureFormat::Bgra8UnormSrgb,
        Some(R8Unorm) => wgpu::TextureFormat::R8Unorm,
        Some(R8Snorm) => wgpu::TextureFormat::R8Snorm,
        Some(R8Uint) => wgpu::TextureFormat::R8Uint,
        Some(R8Sint) => wgpu::TextureFormat::R8Sint,
        Some(R16Unorm) => wgpu::TextureFormat::R16Unorm,
        Some(R16Snorm) => wgpu::TextureFormat::R16Snorm,
        Some(R16Uint) => wgpu::TextureFormat::R16Uint,
        Some(R16Sint) => wgpu::TextureFormat::R16Sint,
        Some(R16Float) => wgpu::TextureFormat::R16Float,
        Some(Rgb10A2Unorm) => wgpu::TextureFormat::Rgb10a2Unorm,
        Some(Rgb10A2Uint) => wgpu::TextureFormat::Rgb10a2Uint,
        Some(Rg11B10Ufloat) => wgpu::TextureFormat::Rg11b10Ufloat,
        None => wgpu::TextureFormat::Depth32Float,
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

            // let mut required_features = wgpu::Features::empty();
            // for enable in &meta.enables {
            //     match enable {
            //         reflection::EnableExtension::F16 => {
            //             required_features |= wgpu::Features::SHADER_F16;
            //         }
            //         reflection::EnableExtension::Subgroups => {
            //             required_features |= wgpu::Features::SUBGROUP;
            //         }
            //     }
            // }

            let required_features =
                (wgpu::Features::SHADER_F16 | wgpu::Features::SUBGROUP) & adapter.features();

            let device_descriptor = DeviceDescriptor {
                required_limits: Limits {
                    // This is needed to support swiftshader
                    max_storage_textures_per_shader_stage: 4,
                    max_storage_buffers_per_shader_stage: adapter
                        .limits()
                        .max_storage_buffers_per_shader_stage,
                    ..Default::default()
                },
                required_features,
                ..Default::default()
            };

            let (device, queue) = adapter.request_device(&device_descriptor).await?;

            wgpu_state
                .device_cache
                .insert(config.clone(), (device.clone(), queue.clone()));

            eprintln!("Device {config} initialized");

            (device, queue)
        }
    };

    let preprocessor_opts = preprocessor::Options {
        module_scope_constants: false,
    };

    let preprocessed = preprocessor::preprocess(preprocessor_opts, shader.to_owned());

    let shader_module = ErrorScope::new(
        &device,
        vec![ErrorFilter::Internal, ErrorFilter::Validation],
    )
    .execute(|| {
        device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(Cow::Owned(preprocessed)),
        })
    })
    .await?;

    let mut compute_pipelines = vec![];

    for (ep_name, stage) in &meta.entry_points {
        match stage {
            reflection::ShaderStage::Compute => {
                let pipeline = ErrorScope::new(
                    &device,
                    vec![ErrorFilter::Internal, ErrorFilter::Validation],
                )
                .execute(|| {
                    device.create_compute_pipeline(&ComputePipelineDescriptor {
                        entry_point: Some(ep_name),
                        label: None,
                        module: &shader_module,
                        layout: None,
                        cache: None,
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                    })
                })
                .await?;
                compute_pipelines.push(pipeline);
            }
            reflection::ShaderStage::Vertex => {
                let dummy_module = ErrorScope::new(&device, vec![ErrorFilter::Validation])
                    .execute(|| {
                        device.create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: None,
                            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                                "@fragment fn dummy_fragment() {}",
                            )),
                        })
                    })
                    .await?;

                let _pipeline = ErrorScope::new(
                    &device,
                    vec![ErrorFilter::Internal, ErrorFilter::Validation],
                )
                .execute(|| {
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: None,
                        layout: None,
                        vertex: wgpu::VertexState {
                            module: &shader_module,
                            entry_point: Some(ep_name),
                            compilation_options: Default::default(),
                            buffers: &[],
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &dummy_module,
                            entry_point: Some("dummy_fragment"),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: wgpu::TextureFormat::Rgba8Unorm,
                                blend: None,
                                write_mask: wgpu::ColorWrites::empty(),
                            })],
                        }),
                        primitive: Default::default(),
                        depth_stencil: None,
                        multisample: Default::default(),
                        multiview_mask: None,
                        cache: None,
                    })
                })
                .await?;
            }
            reflection::ShaderStage::Fragment => {
                let dummy_module = ErrorScope::new(&device, vec![ErrorFilter::Validation])
                    .execute(|| {
                        device.create_shader_module(wgpu::ShaderModuleDescriptor {
                            label: None,
                            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                                "@vertex fn dummy_vertex() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }",
                            )),
                        })
                    })
                    .await?;

                let _pipeline = ErrorScope::new(
                    &device,
                    vec![ErrorFilter::Internal, ErrorFilter::Validation],
                )
                .execute(|| {
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: None,
                        layout: None,
                        vertex: wgpu::VertexState {
                            module: &dummy_module,
                            entry_point: Some("dummy_vertex"),
                            compilation_options: Default::default(),
                            buffers: &[],
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader_module,
                            entry_point: Some(ep_name),
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: wgpu::TextureFormat::Rgba8Unorm,
                                blend: None,
                                write_mask: wgpu::ColorWrites::empty(),
                            })],
                        }),
                        primitive: Default::default(),
                        depth_stencil: None,
                        multisample: Default::default(),
                        multiview_mask: None,
                        cache: None,
                    })
                })
                .await?;
            }
        }
    }

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
        Texture {
            group: u32,
            binding: u32,
            _texture: wgpu::Texture,
            view: wgpu::TextureView,
        },
        Sampler {
            group: u32,
            binding: u32,
            sampler: wgpu::Sampler,
        },
    }

    for resource in &meta.resources {
        let size = resource.size as u64;
        match &resource.kind {
            ResourceKind::StorageBuffer => {
                let actual_size = size + 2 * CANARY_SIZE;
                let gpu_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Storage GPU Buffer"),
                    usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                    size: actual_size,
                    mapped_at_creation: true,
                });

                {
                    let mut initial_data = vec![CANARY_VAL; actual_size as usize];
                    let payload_start = CANARY_SIZE as usize;
                    let payload_end = payload_start + size as usize;

                    if let Some(init) = resource.init.as_deref() {
                        initial_data[payload_start..payload_end].copy_from_slice(init);
                    } else {
                        initial_data[payload_start..payload_end].fill(0);
                    }

                    gpu_buffer
                        .slice(..)
                        .get_mapped_range_mut()?
                        .copy_from_slice(&initial_data);
                }
                gpu_buffer.unmap();

                let staging_buffer = device.create_buffer(&BufferDescriptor {
                    label: Some("Storage Staging Buffer"),
                    usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                    size: actual_size,
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
                        .get_mapped_range_mut()?
                        .copy_from_slice(init);
                }

                buffer.unmap();

                resource_buffers.push(ResourceBuffer::Uniform {
                    group: resource.group,
                    binding: resource.binding,
                    buffer,
                });
            }
            ResourceKind::Texture { dim, format } => {
                let dimension = match dim {
                    reflection::TextureDimension::D1 => wgpu::TextureDimension::D1,
                    reflection::TextureDimension::D2
                    | reflection::TextureDimension::D2Array
                    | reflection::TextureDimension::Cube
                    | reflection::TextureDimension::CubeArray => wgpu::TextureDimension::D2,
                    reflection::TextureDimension::D3 => wgpu::TextureDimension::D3,
                };
                let wgpu_format = map_texture_format_wgpu(*format);
                let is_depth = format.is_none();

                let mut usage =
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
                let supports_storage = matches!(
                    format,
                    Some(
                        reflection::TextureFormat::Rgba8Unorm
                            | reflection::TextureFormat::Rgba8Snorm
                            | reflection::TextureFormat::Rgba8Uint
                            | reflection::TextureFormat::Rgba8Sint
                            | reflection::TextureFormat::Rgba16Uint
                            | reflection::TextureFormat::Rgba16Sint
                            | reflection::TextureFormat::Rgba16Float
                            | reflection::TextureFormat::R32Uint
                            | reflection::TextureFormat::R32Sint
                            | reflection::TextureFormat::R32Float
                            | reflection::TextureFormat::Rg32Uint
                            | reflection::TextureFormat::Rg32Sint
                            | reflection::TextureFormat::Rg32Float
                            | reflection::TextureFormat::Rgba32Uint
                            | reflection::TextureFormat::Rgba32Sint
                            | reflection::TextureFormat::Rgba32Float
                            | reflection::TextureFormat::Bgra8Unorm
                    )
                );
                if supports_storage {
                    usage |= wgpu::TextureUsages::STORAGE_BINDING;
                }

                let layers = if matches!(
                    dim,
                    reflection::TextureDimension::Cube | reflection::TextureDimension::CubeArray
                ) {
                    6
                } else {
                    1
                };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Texture"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: layers,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension,
                    format: wgpu_format,
                    usage,
                    view_formats: &[],
                });

                let view = texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(match dim {
                        reflection::TextureDimension::D1 => wgpu::TextureViewDimension::D1,
                        reflection::TextureDimension::D2 => wgpu::TextureViewDimension::D2,
                        reflection::TextureDimension::D2Array => {
                            wgpu::TextureViewDimension::D2Array
                        }
                        reflection::TextureDimension::Cube => wgpu::TextureViewDimension::Cube,
                        reflection::TextureDimension::CubeArray => {
                            wgpu::TextureViewDimension::CubeArray
                        }
                        reflection::TextureDimension::D3 => wgpu::TextureViewDimension::D3,
                    }),
                    ..Default::default()
                });

                if !is_depth {
                    let data = vec![1u8; 16 * layers as usize];
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &data,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(16),
                            rows_per_image: Some(1),
                        },
                        wgpu::Extent3d {
                            width: 1,
                            height: 1,
                            depth_or_array_layers: layers,
                        },
                    );
                }

                resource_buffers.push(ResourceBuffer::Texture {
                    group: resource.group,
                    binding: resource.binding,
                    _texture: texture,
                    view,
                });
            }
            ResourceKind::Sampler { kind } => {
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("Sampler"),
                    address_mode_u: wgpu::AddressMode::ClampToEdge,
                    address_mode_v: wgpu::AddressMode::ClampToEdge,
                    address_mode_w: wgpu::AddressMode::ClampToEdge,
                    mag_filter: wgpu::FilterMode::Nearest,
                    min_filter: wgpu::FilterMode::Nearest,
                    mipmap_filter: wgpu::MipmapFilterMode::Nearest,
                    compare: if matches!(kind, reflection::SamplerKind::Comparison) {
                        Some(wgpu::CompareFunction::LessEqual)
                    } else {
                        None
                    },
                    ..Default::default()
                });

                resource_buffers.push(ResourceBuffer::Sampler {
                    group: resource.group,
                    binding: resource.binding,
                    sampler,
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
                size,
                gpu_buffer,
                ..
            } => {
                let binding_resource = wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: gpu_buffer,
                    offset: CANARY_SIZE,
                    size: wgpu::BufferSize::new(*size),
                });
                (*group, *binding, binding_resource)
            }
            ResourceBuffer::Uniform {
                group,
                binding,
                buffer,
            } => (*group, *binding, buffer.as_entire_binding()),
            ResourceBuffer::Texture {
                group,
                binding,
                view,
                ..
            } => (*group, *binding, wgpu::BindingResource::TextureView(view)),
            ResourceBuffer::Sampler {
                group,
                binding,
                sampler,
            } => (*group, *binding, wgpu::BindingResource::Sampler(sampler)),
        };

        groups.insert(group);
        bind_groups
            .entry(group)
            .or_insert_with(Vec::new)
            .push(BindGroupEntry { binding, resource });
    }

    let mut final_bind_groups = HashMap::new();

    if let Some(pipeline) = compute_pipelines.first() {
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
    }

    let commands = {
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());

        if !compute_pipelines.is_empty() {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            for pipeline in &compute_pipelines {
                pass.set_pipeline(pipeline);
                for (group, bind_group) in &final_bind_groups {
                    pass.set_bind_group(*group, bind_group, &[]);
                }
                pass.dispatch_workgroups(1, 1, 1);
            }
        }

        for res in &resource_buffers {
            if let ResourceBuffer::Storage {
                size,
                gpu_buffer,
                staging_buffer,
                ..
            } = res
            {
                let actual_size = *size + 2 * CANARY_SIZE;
                encoder.copy_buffer_to_buffer(gpu_buffer, 0, staging_buffer, 0, actual_size);
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
        if let ResourceBuffer::Storage {
            staging_buffer,
            size,
            ..
        } = res
        {
            let slice = staging_buffer.slice(..);
            let (tx, rx) = futures::channel::oneshot::channel();

            slice.map_async(MapMode::Read, move |res| {
                // ignore send errors if receiver dropped
                let _ = tx.send(res);
            });

            pending_mappings.push((rx, slice, staging_buffer, *size));
        }
    }

    device.poll(Wait {
        submission_index: Some(submission_index),
        timeout: None,
    })?;

    let mut results = vec![];

    for (rx, slice, raw_buffer, size) in pending_mappings {
        let map_result = rx.await?;
        map_result?; // propagate mapping errors

        let view = slice.get_mapped_range()?;

        let left_canary = &view[..CANARY_SIZE as usize];
        let right_canary = &view[(CANARY_SIZE as usize + size as usize)..];

        if let Some(pos) = left_canary.iter().position(|&b| b != CANARY_VAL) {
            return Err(eyre!(
                "OOB write detected in config {}: left canary corrupted at relative offset {}. Expected 0x{:02X}, found 0x{:02X}",
                config, pos, CANARY_VAL, left_canary[pos]
            ));
        }

        if let Some(pos) = right_canary.iter().position(|&b| b != CANARY_VAL) {
            return Err(eyre!(
                "OOB write detected in config {}: right canary corrupted at relative offset {}. Expected 0x{:02X}, found 0x{:02X}",
                config, pos, CANARY_VAL, right_canary[pos]
            ));
        }

        results.push(view[(CANARY_SIZE as usize)..(CANARY_SIZE as usize + size as usize)].to_vec());

        drop(view);
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
