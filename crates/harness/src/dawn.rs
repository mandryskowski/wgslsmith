use color_eyre::eyre::eyre;
use dawn::webgpu::{
    WGPUBackendType_WGPUBackendType_D3D12, WGPUBackendType_WGPUBackendType_Metal,
    WGPUBackendType_WGPUBackendType_Vulkan,
};
use dawn::*;
use reflection::{PipelineDescription, ResourceKind};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ConfigId;

const CANARY_SIZE: usize = 256;
const CANARY_VAL: u8 = 0xCD;

type DeviceCacheEntry = (Rc<Device<'static>>, Rc<DeviceQueue>);
pub struct DawnState {
    instance: &'static Instance,
    device_cache: HashMap<ConfigId, DeviceCacheEntry>,
    flags: crate::DawnFlags,
}

impl DawnState {
    pub(crate) fn new(flags: crate::DawnFlags) -> Self {
        let instance = Box::new(Instance::new());
        let instance_ref = Box::leak(instance);

        DawnState {
            instance: instance_ref,
            device_cache: HashMap::new(),
            flags,
        }
    }
}

enum BufferSet {
    Storage {
        group: u32,
        binding: u32,
        size: usize,
        storage: DeviceBuffer,
        read: DeviceBuffer,
    },
    Uniform {
        group: u32,
        binding: u32,
        size: usize,
        buffer: DeviceBuffer,
    },
    Texture {
        group: u32,
        binding: u32,
        _texture: DeviceTexture,
        view: DeviceTextureView,
    },
    Sampler {
        group: u32,
        binding: u32,
        sampler: DeviceSampler,
    },
}

fn map_texture_format_dawn(
    format: &Option<reflection::TextureFormat>,
) -> dawn::webgpu::WGPUTextureFormat {
    use reflection::TextureFormat::*;
    match format {
        Some(Rgba8Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm,
        Some(Rgba8Snorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA8Snorm,
        Some(Rgba8Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA8Uint,
        Some(Rgba8Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA8Sint,
        Some(Rgba16Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA16Unorm,
        Some(Rgba16Snorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA16Snorm,
        Some(Rgba16Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA16Uint,
        Some(Rgba16Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA16Sint,
        Some(Rgba16Float) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA16Float,
        Some(Rg8Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG8Unorm,
        Some(Rg8Snorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG8Snorm,
        Some(Rg8Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG8Uint,
        Some(Rg8Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG8Sint,
        Some(Rg16Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG16Unorm,
        Some(Rg16Snorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG16Snorm,
        Some(Rg16Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG16Uint,
        Some(Rg16Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG16Sint,
        Some(Rg16Float) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG16Float,
        Some(R32Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R32Uint,
        Some(R32Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R32Sint,
        Some(R32Float) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R32Float,
        Some(Rg32Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG32Uint,
        Some(Rg32Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG32Sint,
        Some(Rg32Float) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG32Float,
        Some(Rgba32Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA32Uint,
        Some(Rgba32Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA32Sint,
        Some(Rgba32Float) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGBA32Float,
        Some(Bgra8Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_BGRA8Unorm,
        Some(Bgra8UnormSrgb) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_BGRA8UnormSrgb,
        Some(R8Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R8Unorm,
        Some(R8Snorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R8Snorm,
        Some(R8Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R8Uint,
        Some(R8Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R8Sint,
        Some(R16Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R16Unorm,
        Some(R16Snorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R16Snorm,
        Some(R16Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R16Uint,
        Some(R16Sint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R16Sint,
        Some(R16Float) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_R16Float,
        Some(Rgb10A2Unorm) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGB10A2Unorm,
        Some(Rgb10A2Uint) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RGB10A2Uint,
        Some(Rg11B10Ufloat) => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_RG11B10Ufloat,
        None => dawn::webgpu::WGPUTextureFormat_WGPUTextureFormat_Depth32Float,
    }
}

pub fn get_adapters() -> Vec<types::Adapter> {
    Instance::new()
        .enumerate_adapters()
        .into_iter()
        .filter_map(|it| {
            #[allow(non_upper_case_globals)]
            Some(types::Adapter {
                name: it.name,
                device_id: it.device_id,
                backend: match it.backend {
                    WGPUBackendType_WGPUBackendType_D3D12 => crate::BackendType::Dx12,
                    WGPUBackendType_WGPUBackendType_Metal => crate::BackendType::Metal,
                    WGPUBackendType_WGPUBackendType_Vulkan => crate::BackendType::Vulkan,
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
    dawn_state: Option<&mut DawnState>,
) -> color_eyre::Result<Vec<Vec<u8>>> {
    let backend = match config.backend {
        crate::BackendType::Dx12 => WGPUBackendType_WGPUBackendType_D3D12,
        crate::BackendType::Metal => WGPUBackendType_WGPUBackendType_Metal,
        crate::BackendType::Vulkan => WGPUBackendType_WGPUBackendType_Vulkan,
    };

    let mut _owned_dawn_state;
    let dawn_state: &mut DawnState = match dawn_state {
        Some(state) => state,
        None => {
            let default_flags = crate::DawnFlags {
                enabled: vec!["use_dxc".to_owned()],
                disabled: vec![],
            };
            _owned_dawn_state = DawnState::new(default_flags);
            &mut _owned_dawn_state
        }
    };

    let (device, queue) = {
        if let Some((cached_device, cached_queue)) = dawn_state.device_cache.get(config) {
            (cached_device.clone(), cached_queue.clone())
        } else {
            let mut required_features = vec![];
            // for enable in &meta.enables {
            //     match enable {
            //         reflection::EnableExtension::F16 => {
            //             required_features
            //                 .push(dawn::webgpu::WGPUFeatureName_WGPUFeatureName_ShaderF16);
            //         }
            //         reflection::EnableExtension::Subgroups => {
            //             required_features
            //                 .push(dawn::webgpu::WGPUFeatureName_WGPUFeatureName_Subgroups);
            //         }
            //     }
            // }
            required_features = vec![
                dawn::webgpu::WGPUFeatureName_WGPUFeatureName_ShaderF16,
                dawn::webgpu::WGPUFeatureName_WGPUFeatureName_Subgroups,
            ];

            let enabled_refs: Vec<&str> = dawn_state
                .flags
                .enabled
                .iter()
                .map(|s| s.as_str())
                .collect();
            let disabled_refs: Vec<&str> = dawn_state
                .flags
                .disabled
                .iter()
                .map(|s| s.as_str())
                .collect();

            let toggles = dawn::DawnToggles {
                enabled: &enabled_refs,
                disabled: &disabled_refs,
            };

            let device = dawn_state
                .instance
                .create_device(backend, config.device_id, &required_features, &toggles)
                .ok_or_else(|| eyre!("no adapter found matching id: {config}"))?;

            let queue = device.create_queue();

            let device_rc = Rc::new(device);
            let queue_rc = Rc::new(queue);

            dawn_state
                .device_cache
                .insert(config.clone(), (device_rc.clone(), queue_rc.clone()));

            eprintln!("Device {config} initialized");

            (device_rc, queue_rc)
        }
    };

    let instance = dawn_state.instance;

    let shader_module = device.create_shader_module(shader)?;
    let mut compute_pipelines = vec![];

    for (ep_name, stage) in &meta.entry_points {
        match stage {
            reflection::ShaderStage::Compute => {
                let pipeline = device.create_compute_pipeline(&shader_module, ep_name)?;
                compute_pipelines.push(pipeline);
            }
            reflection::ShaderStage::Vertex => {
                let dummy_module =
                    device.create_shader_module("@fragment fn dummy_fragment() {}")?;
                device.create_render_pipeline(
                    &shader_module,
                    ep_name,
                    Some(&dummy_module),
                    Some("dummy_fragment"),
                )?;
            }
            reflection::ShaderStage::Fragment => {
                let dummy_module = device.create_shader_module(
                    "@vertex fn dummy_vertex() -> @builtin(position) vec4<f32> { return vec4<f32>(0.0, 0.0, 0.0, 1.0); }"
                )?;
                device.create_render_pipeline(
                    &dummy_module,
                    "dummy_vertex",
                    Some(&shader_module),
                    Some(ep_name),
                )?;
            }
        }
    }

    let mut buffer_sets = vec![];

    for resource in &meta.resources {
        let size = resource.size as usize;
        match &resource.kind {
            ResourceKind::StorageBuffer => {
                let actual_size = size + 2 * CANARY_SIZE;

                let mut storage = device.create_buffer(
                    1, // mapped: 1
                    actual_size,
                    DeviceBufferUsage::STORAGE | DeviceBufferUsage::COPY_SRC,
                )?;

                {
                    let view = storage.get_mapped_range(actual_size);
                    view[..CANARY_SIZE].fill(CANARY_VAL);
                    view[CANARY_SIZE + size..].fill(CANARY_VAL);

                    if let Some(init) = resource.init.as_deref() {
                        view[CANARY_SIZE..CANARY_SIZE + size].copy_from_slice(init);
                    } else {
                        view[CANARY_SIZE..CANARY_SIZE + size].fill(0);
                    }
                }
                storage.unmap();

                let read = device.create_buffer(
                    0,
                    actual_size,
                    DeviceBufferUsage::COPY_DST | DeviceBufferUsage::MAP_READ,
                )?;

                buffer_sets.push(BufferSet::Storage {
                    group: resource.group,
                    binding: resource.binding,
                    size,
                    storage,
                    read,
                });
            }
            ResourceKind::UniformBuffer => {
                let mut buffer = device.create_buffer(1, size, DeviceBufferUsage::UNIFORM)?;

                {
                    let view = buffer.get_mapped_range(size);
                    if let Some(init) = resource.init.as_deref() {
                        view.copy_from_slice(init);
                    } else {
                        view.fill(0);
                    }
                }

                buffer.unmap();

                buffer_sets.push(BufferSet::Uniform {
                    group: resource.group,
                    binding: resource.binding,
                    size,
                    buffer,
                })
            }
            ResourceKind::Texture { dim, format } => {
                let dimension = match dim {
                    reflection::TextureDimension::D1 => {
                        dawn::webgpu::WGPUTextureDimension_WGPUTextureDimension_1D
                    }
                    reflection::TextureDimension::D2
                    | reflection::TextureDimension::D2Array
                    | reflection::TextureDimension::Cube
                    | reflection::TextureDimension::CubeArray => {
                        dawn::webgpu::WGPUTextureDimension_WGPUTextureDimension_2D
                    }
                    reflection::TextureDimension::D3 => {
                        dawn::webgpu::WGPUTextureDimension_WGPUTextureDimension_3D
                    }
                };
                let wgpu_format = map_texture_format_dawn(format);
                let is_depth = format.is_none();
                let mut usage = DeviceTextureUsage::TEXTURE_BINDING | DeviceTextureUsage::COPY_DST;

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
                    usage |= DeviceTextureUsage::STORAGE_BINDING;
                }

                let layers = if matches!(
                    dim,
                    reflection::TextureDimension::Cube | reflection::TextureDimension::CubeArray
                ) {
                    6
                } else {
                    1
                };
                let texture = device.create_texture(
                    wgpu_format,
                    usage,
                    dimension,
                    dawn::webgpu::WGPUExtent3D {
                        width: 1,
                        height: 1,
                        depthOrArrayLayers: layers,
                    },
                )?;

                let view = unsafe {
                    let mut desc: dawn::webgpu::WGPUTextureViewDescriptor = std::mem::zeroed();
                    desc.format = wgpu_format;
                    desc.dimension = match dim {
                        reflection::TextureDimension::D1 => dawn::webgpu::WGPUTextureViewDimension_WGPUTextureViewDimension_1D,
                        reflection::TextureDimension::D2 => dawn::webgpu::WGPUTextureViewDimension_WGPUTextureViewDimension_2D,
                        reflection::TextureDimension::D2Array => dawn::webgpu::WGPUTextureViewDimension_WGPUTextureViewDimension_2DArray,
                        reflection::TextureDimension::Cube => dawn::webgpu::WGPUTextureViewDimension_WGPUTextureViewDimension_Cube,
                        reflection::TextureDimension::CubeArray => dawn::webgpu::WGPUTextureViewDimension_WGPUTextureViewDimension_CubeArray,
                        reflection::TextureDimension::D3 => dawn::webgpu::WGPUTextureViewDimension_WGPUTextureViewDimension_3D,
                    };
                    desc.baseMipLevel = 0;
                    desc.mipLevelCount = 1;
                    desc.baseArrayLayer = 0;
                    desc.arrayLayerCount = layers;
                    desc.aspect = dawn::webgpu::WGPUTextureAspect_WGPUTextureAspect_All;
                    dawn::DeviceTextureView {
                        handle: dawn::webgpu::wgpuTextureCreateView(texture.handle, &desc),
                    }
                };

                if !is_depth {
                    let data = vec![1u8; 16 * layers as usize];
                    queue.write_texture(
                        &dawn::webgpu::WGPUTexelCopyTextureInfo {
                            texture: texture.handle,
                            mipLevel: 0,
                            origin: dawn::webgpu::WGPUOrigin3D { x: 0, y: 0, z: 0 },
                            aspect: dawn::webgpu::WGPUTextureAspect_WGPUTextureAspect_All,
                        },
                        &data,
                        &dawn::webgpu::WGPUTexelCopyBufferLayout {
                            offset: 0,
                            bytesPerRow: 16,
                            rowsPerImage: 1,
                        },
                        &dawn::webgpu::WGPUExtent3D {
                            width: 1,
                            height: 1,
                            depthOrArrayLayers: layers,
                        },
                    );
                }

                buffer_sets.push(BufferSet::Texture {
                    group: resource.group,
                    binding: resource.binding,
                    _texture: texture,
                    view,
                });
            }
            ResourceKind::Sampler { kind } => {
                let sampler = device.create_sampler(&dawn::webgpu::WGPUSamplerDescriptor {
                    label: dawn::webgpu::WGPUStringView {
                        data: std::ptr::null(),
                        length: 0,
                    },
                    nextInChain: std::ptr::null_mut(),
                    addressModeU: dawn::webgpu::WGPUAddressMode_WGPUAddressMode_ClampToEdge,
                    addressModeV: dawn::webgpu::WGPUAddressMode_WGPUAddressMode_ClampToEdge,
                    addressModeW: dawn::webgpu::WGPUAddressMode_WGPUAddressMode_ClampToEdge,
                    magFilter: dawn::webgpu::WGPUFilterMode_WGPUFilterMode_Nearest,
                    minFilter: dawn::webgpu::WGPUFilterMode_WGPUFilterMode_Nearest,
                    mipmapFilter: dawn::webgpu::WGPUMipmapFilterMode_WGPUMipmapFilterMode_Nearest,
                    lodMinClamp: 0.0,
                    lodMaxClamp: 32.0,
                    compare: if matches!(kind, reflection::SamplerKind::Comparison) {
                        dawn::webgpu::WGPUCompareFunction_WGPUCompareFunction_LessEqual
                    } else {
                        dawn::webgpu::WGPUCompareFunction_WGPUCompareFunction_Undefined
                    },
                    maxAnisotropy: 1,
                })?;

                buffer_sets.push(BufferSet::Sampler {
                    group: resource.group,
                    binding: resource.binding,
                    sampler,
                });
            }
        }
    }

    let mut bind_groups = HashMap::new();
    let mut groups = HashSet::new();

    for buffer in &buffer_sets {
        let (group, binding, resource, offset, size) = match buffer {
            BufferSet::Storage {
                group,
                binding,
                storage,
                size,
                ..
            } => (
                *group,
                *binding,
                BindGroupEntryResource::Buffer(storage),
                CANARY_SIZE,
                *size,
            ),
            BufferSet::Uniform {
                group,
                binding,
                buffer,
                size,
            } => (
                *group,
                *binding,
                BindGroupEntryResource::Buffer(buffer),
                0,
                *size,
            ),
            BufferSet::Texture {
                group,
                binding,
                view,
                ..
            } => (
                *group,
                *binding,
                BindGroupEntryResource::TextureView(view),
                0,
                0,
            ),
            BufferSet::Sampler {
                group,
                binding,
                sampler,
            } => (
                *group,
                *binding,
                BindGroupEntryResource::Sampler(sampler),
                0,
                0,
            ),
        };

        groups.insert(group);
        bind_groups
            .entry(group)
            .or_insert_with(Vec::new)
            .push(BindGroupEntry {
                binding,
                resource,
                offset,
                size,
            });
    }

    let mut final_bind_groups = HashMap::new();

    if let Some(pipeline) = compute_pipelines.first() {
        for (group, entries) in bind_groups {
            let layout = pipeline.get_bind_group_layout(group);
            let bind_group = device.create_bind_group(&layout, &entries)?;
            final_bind_groups.insert(group, bind_group);
        }
    }

    let encoder = device.create_command_encoder()?;

    if !compute_pipelines.is_empty() {
        let compute_pass = encoder.begin_compute_pass();
        for pipeline in &compute_pipelines {
            compute_pass.set_pipeline(pipeline);
            for (group, bind_group) in &final_bind_groups {
                compute_pass.set_bind_group(*group, bind_group);
            }
            compute_pass.dispatch(1, 1, 1);
        }
    }

    for buffers in &buffer_sets {
        if let BufferSet::Storage {
            storage,
            read,
            size,
            ..
        } = buffers
        {
            let actual_size = *size + 2 * CANARY_SIZE;
            encoder.copy_buffer_to_buffer(storage, read, actual_size);
        }
    }

    let commands = encoder.finish()?;

    queue.submit(&commands);

    let mut results = vec![];
    for buffers in &buffer_sets {
        if let BufferSet::Storage { read, size, .. } = buffers {
            let size = *size;
            let actual_size = size + 2 * CANARY_SIZE;
            let mut rx = read.map_async(DeviceBufferMapMode::READ, actual_size);

            loop {
                match rx.try_recv() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        instance.process_events();
                        std::thread::sleep(std::time::Duration::from_millis(16));
                    }
                    Err(_) => {
                        unmap_device(dawn_state, config);
                        return Err(eyre!("Buffer mapping failed"));
                    }
                }
            }

            let bytes = read.get_const_mapped_range(actual_size);

            let left_canary = &bytes[..CANARY_SIZE];
            let right_canary = &bytes[CANARY_SIZE + size..];

            if let Some(pos) = left_canary.iter().position(|&b| b != CANARY_VAL) {
                unmap_device(dawn_state, config);
                return Err(eyre!(
                    "OOB write detected in config {}: left canary corrupted at relative offset {}. Expected 0x{:02X}, found 0x{:02X}",
                    config, pos, CANARY_VAL, left_canary[pos]
                ));
            }

            if let Some(pos) = right_canary.iter().position(|&b| b != CANARY_VAL) {
                unmap_device(dawn_state, config);
                return Err(eyre!(
                    "OOB write detected in config {}: right canary corrupted at relative offset {}. Expected 0x{:02X}, found 0x{:02X}",
                    config, pos, CANARY_VAL, right_canary[pos]
                ));
            }

            results.push(bytes[CANARY_SIZE..CANARY_SIZE + size].to_vec());
        }
    }

    Ok(results)
}

fn unmap_device(dawn_state: &mut DawnState, config: &ConfigId) {
    eprintln!("Removing device {config}");
    dawn_state.device_cache.remove(config);
}
