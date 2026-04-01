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

            (device_rc, queue_rc)
        }
    };

    let instance = dawn_state.instance;

    let shader_module = device.create_shader_module(shader)?;
    let pipeline = device.create_compute_pipeline(&shader_module, &meta.entry_point)?;

    let mut buffer_sets = vec![];

    for resource in &meta.resources {
        let size = resource.size as usize;
        match resource.kind {
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

                if let Some(init) = resource.init.as_deref() {
                    buffer.get_mapped_range(size).copy_from_slice(init);
                }

                buffer.unmap();

                buffer_sets.push(BufferSet::Uniform {
                    group: resource.group,
                    binding: resource.binding,
                    size,
                    buffer,
                })
            }
            ResourceKind::Texture { .. } | ResourceKind::Sampler { .. } => todo!(),
        }
    }

    let mut bind_groups = HashMap::new();
    let mut groups = HashSet::new();

    for buffer in &buffer_sets {
        let (group, binding, buffer_obj, offset, size) = match buffer {
            BufferSet::Storage {
                group,
                binding,
                storage,
                size,
                ..
            } => (*group, *binding, storage, CANARY_SIZE, *size),
            BufferSet::Uniform {
                group,
                binding,
                buffer,
                size,
            } => (*group, *binding, buffer, 0, *size),
        };

        groups.insert(group);
        bind_groups
            .entry(group)
            .or_insert_with(Vec::new)
            .push(BindGroupEntry {
                binding,
                resource: BindGroupEntryResource::Buffer(buffer_obj),
                offset,
                size,
            });
    }

    let bind_groups: HashMap<_, _> = bind_groups
        .into_iter()
        .map(|(group, entries)| {
            (
                group,
                device
                    .create_bind_group(&pipeline.get_bind_group_layout(group), &entries)
                    .unwrap(),
            )
        })
        .collect();

    let encoder = device.create_command_encoder()?;

    {
        let compute_pass = encoder.begin_compute_pass();
        compute_pass.set_pipeline(&pipeline);
        for (group, bind_group) in &bind_groups {
            compute_pass.set_bind_group(*group, bind_group);
        }
        compute_pass.dispatch(1, 1, 1);
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
