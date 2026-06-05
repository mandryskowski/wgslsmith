use crate::dawn;
use crate::webgpu::*;
use eyre::{eyre, Result};
use futures::channel::oneshot;
use harness_types::BackendValidationLevel;
use std::ffi::c_void;
use std::mem::zeroed;
use std::ptr::{null, null_mut};

fn make_string_view(value: &str) -> WGPUStringView {
    WGPUStringView {
        data: value.as_ptr().cast(),
        length: value.len(),
    }
}

pub struct Instance(*mut c_void);

pub struct AdapterInfo {
    pub name: String,
    pub backend: WGPUBackendType,
    pub device_id: u32,
}

#[derive(Clone, Default, Debug)]
pub struct DawnToggles<'a> {
    pub enabled: &'a [&'a str],
    pub disabled: &'a [&'a str],
}

impl Instance {
    pub fn new(level: BackendValidationLevel) -> Instance {
        Instance(unsafe { dawn::new_instance(level as i32) })
    }

    pub fn process_events(&self) {
        unsafe {
            dawn::instance_process_events(self.0);
        }
    }

    pub fn enumerate_adapters(&self) -> Vec<AdapterInfo> {
        #[allow(non_upper_case_globals)]
        unsafe extern "C" fn cb(info: *const WGPUAdapterInfo, userdata: *mut c_void) {
            let info_ref = info.as_ref().unwrap();
            let name_str = if !info_ref.device.data.is_null() {
                let slice = std::slice::from_raw_parts(
                    info_ref.device.data as *const u8,
                    info_ref.device.length,
                );
                String::from_utf8_lossy(slice).into_owned()
            } else {
                "Unknown Adapter".to_owned()
            };

            (userdata as *mut Vec<AdapterInfo>)
                .as_mut()
                .unwrap()
                .push(AdapterInfo {
                    name: name_str,
                    backend: (*info).backendType,
                    device_id: (*info).deviceID,
                });
        }

        let mut adapters = vec![];

        unsafe {
            dawn::enumerate_adapters(self.0, Some(cb), &mut adapters as *mut _ as *mut c_void);
        }

        adapters
    }

    pub fn create_device(
        &self,
        backend: WGPUBackendType,
        device_id: u32,
        enables: &[WGPUFeatureName],
        toggles: &DawnToggles<'_>,
    ) -> Option<Device<'_>> {
        let c_enabled: Vec<std::ffi::CString> = toggles
            .enabled
            .iter()
            .map(|&t| std::ffi::CString::new(t).unwrap())
            .collect();
        let c_enabled_ptrs: Vec<*const std::os::raw::c_char> =
            c_enabled.iter().map(|c| c.as_ptr() as _).collect();

        let c_disabled: Vec<std::ffi::CString> = toggles
            .disabled
            .iter()
            .map(|&t| std::ffi::CString::new(t).unwrap())
            .collect();
        let c_disabled_ptrs: Vec<*const std::os::raw::c_char> =
            c_disabled.iter().map(|c| c.as_ptr() as _).collect();

        let callback: WGPUUncapturedErrorCallback = Some(default_error_callback);
        let handle = unsafe {
            dawn::create_device(
                self.0,
                backend,
                device_id,
                callback,
                null_mut(),
                enables.as_ptr(),
                enables.len(),
                c_enabled_ptrs.as_ptr(),
                c_enabled_ptrs.len(),
                c_disabled_ptrs.as_ptr(),
                c_disabled_ptrs.len(),
            )
        };

        if handle.is_null() {
            panic!("failed to create dawn device");
        }

        let device = Device {
            _instance: self,
            handle,
        };

        Some(device)
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self::new(harness_types::BackendValidationLevel::default())
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            dawn::delete_instance(self.0);
        }
    }
}

pub struct Device<'a> {
    // The _instance field ensures that a Device does not outlive its Instance.
    _instance: &'a Instance,
    handle: *mut crate::webgpu::WGPUDeviceImpl,
}

impl Device<'_> {
    pub fn create_queue(&self) -> DeviceQueue {
        DeviceQueue {
            handle: unsafe { wgpuDeviceGetQueue(self.handle).assert_not_null() },
        }
    }

    pub fn create_shader_module(&self, source: &str) -> Result<ShaderModule> {
        ErrorScope::new(self, "shader module creation failed").execute(|| unsafe {
            let wgsl_descriptor = WGPUShaderSourceWGSL {
                chain: WGPUChainedStruct {
                    sType: WGPUSType_WGPUSType_ShaderSourceWGSL,
                    ..zeroed()
                },
                code: make_string_view(source),
            };

            let descriptor = WGPUShaderModuleDescriptor {
                nextInChain: &wgsl_descriptor as *const _ as _,
                ..zeroed()
            };

            ShaderModule {
                handle: wgpuDeviceCreateShaderModule(self.handle, &descriptor).assert_not_null(),
            }
        })
    }

    pub fn create_compute_pipeline(
        &self,
        shader_module: &ShaderModule,
        entrypoint: &str,
    ) -> Result<ComputePipeline> {
        ErrorScope::new(self, "compute pipeline creation failed").execute(|| unsafe {
            ComputePipeline {
                handle: wgpuDeviceCreateComputePipeline(
                    self.handle,
                    &WGPUComputePipelineDescriptor {
                        label: make_string_view(format!("Pipeline: {entrypoint}").as_str()),
                        nextInChain: null_mut(),
                        layout: null_mut(),
                        compute: WGPUComputeState {
                            constantCount: 0,
                            constants: null(),
                            module: shader_module.handle,
                            entryPoint: make_string_view(entrypoint),
                            nextInChain: null_mut(),
                        },
                    },
                ),
            }
        })
    }

    pub fn create_render_pipeline(
        &self,
        vertex_module: &ShaderModule,
        vertex_entry: &str,
        fragment_module: Option<&ShaderModule>,
        fragment_entry: Option<&str>,
    ) -> Result<RenderPipeline> {
        ErrorScope::new(self, "render pipeline creation failed").execute(|| unsafe {
            let mut descriptor: WGPURenderPipelineDescriptor = zeroed();
            descriptor.label = make_string_view("Render Pipeline");

            descriptor.vertex.module = vertex_module.handle;
            descriptor.vertex.entryPoint = make_string_view(vertex_entry);
            descriptor.vertex.bufferCount = 0;
            descriptor.vertex.buffers = null();

            let mut fragment_state: WGPUFragmentState = zeroed();
            let mut color_target: WGPUColorTargetState = zeroed();

            if let (Some(frag_mod), Some(frag_ep)) = (fragment_module, fragment_entry) {
                color_target.format = WGPUTextureFormat_WGPUTextureFormat_RGBA8Unorm;
                color_target.writeMask = 0; // WGPUColorWriteMask_None

                fragment_state.module = frag_mod.handle;
                fragment_state.entryPoint = make_string_view(frag_ep);
                fragment_state.targetCount = 1;
                fragment_state.targets = &color_target;

                descriptor.fragment = &fragment_state;
            } else {
                descriptor.fragment = null();
            }

            descriptor.primitive.topology =
                WGPUPrimitiveTopology_WGPUPrimitiveTopology_TriangleList;
            descriptor.primitive.cullMode = WGPUCullMode_WGPUCullMode_None;
            descriptor.primitive.frontFace = WGPUFrontFace_WGPUFrontFace_CCW;

            descriptor.multisample.count = 1;
            descriptor.multisample.mask = !0;

            RenderPipeline {
                handle: wgpuDeviceCreateRenderPipeline(self.handle, &descriptor).assert_not_null(),
            }
        })
    }

    pub fn create_buffer(
        &self,
        mapped: crate::webgpu::WGPUBool,
        size: usize,
        usage: DeviceBufferUsage,
    ) -> Result<DeviceBuffer> {
        ErrorScope::new(self, "buffer creation failed").execute(|| unsafe {
            DeviceBuffer {
                handle: wgpuDeviceCreateBuffer(
                    self.handle,
                    &WGPUBufferDescriptor {
                        label: WGPUStringView {
                            data: null(),
                            length: 0,
                        },
                        nextInChain: null_mut(),
                        mappedAtCreation: mapped,
                        size: size as _,
                        usage: usage.bits as _,
                    },
                )
                .assert_not_null(),
            }
        })
    }

    pub fn create_bind_group(
        &self,
        layout: &BindGroupLayout,
        entries: &[BindGroupEntry],
    ) -> Result<BindGroup> {
        ErrorScope::new(self, "bind group creation failed").execute(|| unsafe {
            let entries = entries.iter().map(|e| e.into()).collect::<Vec<_>>();
            BindGroup {
                handle: wgpuDeviceCreateBindGroup(
                    self.handle,
                    &WGPUBindGroupDescriptor {
                        label: WGPUStringView {
                            data: null(),
                            length: 0,
                        },
                        nextInChain: null_mut(),
                        layout: layout.handle,
                        entries: entries.as_ptr(),
                        entryCount: entries.len() as _,
                    },
                )
                .assert_not_null(),
            }
        })
    }

    pub fn create_command_encoder(&self) -> Result<CommandEncoder<'_>> {
        ErrorScope::new(self, "command encoder creation failed").execute(|| unsafe {
            CommandEncoder {
                device: self,
                handle: wgpuDeviceCreateCommandEncoder(self.handle, &zeroed()).assert_not_null(),
            }
        })
    }

    pub fn create_texture(
        &self,
        format: WGPUTextureFormat,
        usage: DeviceTextureUsage,
        dimension: WGPUTextureDimension,
        size: WGPUExtent3D,
    ) -> Result<DeviceTexture> {
        ErrorScope::new(self, "texture creation failed").execute(|| unsafe {
            DeviceTexture {
                handle: wgpuDeviceCreateTexture(
                    self.handle,
                    &WGPUTextureDescriptor {
                        label: WGPUStringView {
                            data: null(),
                            length: 0,
                        },
                        nextInChain: null_mut(),
                        usage: usage.bits as _,
                        dimension,
                        size,
                        format,
                        mipLevelCount: 1,
                        sampleCount: 1,
                        viewFormatCount: 0,
                        viewFormats: null(),
                    },
                )
                .assert_not_null(),
            }
        })
    }

    pub fn create_sampler(&self, descriptor: &WGPUSamplerDescriptor) -> Result<DeviceSampler> {
        ErrorScope::new(self, "sampler creation failed").execute(|| unsafe {
            DeviceSampler {
                handle: wgpuDeviceCreateSampler(self.handle, descriptor).assert_not_null(),
            }
        })
    }

    pub fn tick(&self) {
        unsafe {
            wgpuDeviceTick(self.handle);
        }
    }
}

impl Drop for Device<'_> {
    fn drop(&mut self) {
        unsafe {
            eprintln!("Dropping device");
            wgpuDeviceDestroy(self.handle);
            self._instance.process_events()
        }
    }
}

pub struct DeviceQueue {
    handle: WGPUQueue,
}

impl DeviceQueue {
    pub fn submit(&self, commands: &CommandBuffer) {
        unsafe {
            wgpuQueueSubmit(self.handle, 1, &commands.handle);
        }
    }

    pub fn write_texture(
        &self,
        destination: &WGPUTexelCopyTextureInfo,
        data: &[u8],
        data_layout: &WGPUTexelCopyBufferLayout,
        write_size: &WGPUExtent3D,
    ) {
        unsafe {
            wgpuQueueWriteTexture(
                self.handle,
                destination,
                data.as_ptr() as _,
                data.len(),
                data_layout,
                write_size,
            );
        }
    }
}

impl Drop for DeviceQueue {
    fn drop(&mut self) {
        unsafe {
            wgpuQueueRelease(self.handle);
        }
    }
}

pub struct ShaderModule {
    handle: WGPUShaderModule,
}

impl Drop for ShaderModule {
    fn drop(&mut self) {
        unsafe {
            wgpuShaderModuleRelease(self.handle);
        }
    }
}

pub struct RenderPipeline {
    handle: WGPURenderPipeline,
}

impl Drop for RenderPipeline {
    fn drop(&mut self) {
        unsafe {
            wgpuRenderPipelineRelease(self.handle);
        }
    }
}

pub struct ComputePipeline {
    handle: WGPUComputePipeline,
}

impl ComputePipeline {
    pub fn get_bind_group_layout(&self, index: u32) -> BindGroupLayout {
        unsafe {
            BindGroupLayout {
                handle: wgpuComputePipelineGetBindGroupLayout(self.handle, index).assert_not_null(),
            }
        }
    }
}

impl Drop for ComputePipeline {
    fn drop(&mut self) {
        unsafe {
            wgpuComputePipelineRelease(self.handle);
        }
    }
}

pub struct DeviceBuffer {
    handle: WGPUBuffer,
}

bitflags::bitflags! {
    pub struct DeviceBufferUsage: WGPUBufferUsage {
        const STORAGE = WGPUBufferUsage_Storage;
        const UNIFORM = WGPUBufferUsage_Uniform;
        const COPY_SRC = WGPUBufferUsage_CopySrc;
        const COPY_DST = WGPUBufferUsage_CopyDst;
        const MAP_READ = WGPUBufferUsage_MapRead;
    }
}

bitflags::bitflags! {
    pub struct DeviceBufferMapMode: WGPUMapMode {
        const READ = WGPUMapMode_Read;
    }
}

impl DeviceBuffer {
    pub fn map_async(&self, mode: DeviceBufferMapMode, size: usize) -> oneshot::Receiver<()> {
        unsafe {
            unsafe extern "C" fn map_callback(
                res: WGPUMapAsyncStatus,
                message: WGPUStringView,
                userdata1: *mut c_void,
                _userdata2: *mut c_void,
            ) {
                let mut tx = Box::from_raw(userdata1 as *mut Option<oneshot::Sender<()>>);

                if res == WGPUMapAsyncStatus_WGPUMapAsyncStatus_Success {
                    if let Some(sender) = (*tx).take() {
                        let _ = sender.send(());
                    }
                } else {
                    let msg_str = if !message.data.is_null() {
                        let slice =
                            std::slice::from_raw_parts(message.data as *const u8, message.length);
                        String::from_utf8_lossy(slice)
                    } else {
                        "Unknown error".into()
                    };

                    eprintln!(
                        "wgpuBufferMapAsync failed status: {} message: {}",
                        res, msg_str
                    );
                }
            }

            let (tx, rx) = oneshot::channel::<()>();
            let tx = Box::new(Some(tx));

            let callback_info = WGPUBufferMapCallbackInfo {
                nextInChain: null_mut(),
                mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
                callback: Some(map_callback),
                userdata1: Box::into_raw(tx) as _,
                userdata2: null_mut(),
            };

            wgpuBufferMapAsync(self.handle, mode.bits as _, 0, size as _, callback_info);

            rx
        }
    }

    pub fn get_mapped_range(&mut self, size: usize) -> &mut [u8] {
        unsafe {
            let ptr = wgpuBufferGetMappedRange(self.handle, 0, size as _);
            std::slice::from_raw_parts_mut(ptr as _, size)
        }
    }

    pub fn get_const_mapped_range(&self, size: usize) -> &[u8] {
        unsafe {
            let ptr = wgpuBufferGetConstMappedRange(self.handle, 0, size as _);
            std::slice::from_raw_parts(ptr as _, size)
        }
    }

    pub fn unmap(&self) {
        unsafe {
            wgpuBufferUnmap(self.handle);
        }
    }
}

impl Drop for DeviceBuffer {
    fn drop(&mut self) {
        unsafe {
            wgpuBufferRelease(self.handle);
        }
    }
}

pub struct DeviceTexture {
    pub handle: WGPUTexture,
}

impl DeviceTexture {
    pub fn create_view(&self) -> DeviceTextureView {
        unsafe {
            DeviceTextureView {
                handle: wgpuTextureCreateView(self.handle, null()).assert_not_null(),
            }
        }
    }
}

impl Drop for DeviceTexture {
    fn drop(&mut self) {
        unsafe {
            wgpuTextureRelease(self.handle);
        }
    }
}

pub struct DeviceTextureView {
    pub handle: WGPUTextureView,
}

impl Drop for DeviceTextureView {
    fn drop(&mut self) {
        unsafe {
            wgpuTextureViewRelease(self.handle);
        }
    }
}

pub struct DeviceSampler {
    pub handle: WGPUSampler,
}

impl Drop for DeviceSampler {
    fn drop(&mut self) {
        unsafe {
            wgpuSamplerRelease(self.handle);
        }
    }
}

bitflags::bitflags! {
    pub struct DeviceTextureUsage: WGPUTextureUsage {
        const COPY_SRC = WGPUTextureUsage_CopySrc;
        const COPY_DST = WGPUTextureUsage_CopyDst;
        const TEXTURE_BINDING = WGPUTextureUsage_TextureBinding;
        const STORAGE_BINDING = WGPUTextureUsage_StorageBinding;
        const RENDER_ATTACHMENT = WGPUTextureUsage_RenderAttachment;
    }
}

pub struct BindGroupLayout {
    handle: WGPUBindGroupLayout,
}

impl BindGroupLayout {}

impl Drop for BindGroupLayout {
    fn drop(&mut self) {
        unsafe {
            wgpuBindGroupLayoutRelease(self.handle);
        }
    }
}

pub struct BindGroupEntry<'a> {
    pub binding: u32,
    pub resource: BindGroupEntryResource<'a>,
    pub offset: usize,
    pub size: usize,
}

pub enum BindGroupEntryResource<'a> {
    Buffer(&'a DeviceBuffer),
    TextureView(&'a DeviceTextureView),
    Sampler(&'a DeviceSampler),
}

impl<'a> From<&BindGroupEntry<'a>> for WGPUBindGroupEntry {
    fn from(entry: &BindGroupEntry<'a>) -> Self {
        let (buffer, texture_view, sampler) = match entry.resource {
            BindGroupEntryResource::Buffer(b) => (b.handle, null_mut(), null_mut()),
            BindGroupEntryResource::TextureView(t) => (null_mut(), t.handle, null_mut()),
            BindGroupEntryResource::Sampler(s) => (null_mut(), null_mut(), s.handle),
        };
        WGPUBindGroupEntry {
            binding: entry.binding,
            buffer,
            offset: entry.offset as _,
            size: entry.size as _,
            sampler,
            textureView: texture_view,
            nextInChain: null_mut(),
        }
    }
}

pub struct BindGroup {
    handle: WGPUBindGroup,
}

impl BindGroup {}

impl Drop for BindGroup {
    fn drop(&mut self) {
        unsafe {
            wgpuBindGroupRelease(self.handle);
        }
    }
}

pub struct CommandEncoder<'a> {
    device: &'a Device<'a>,
    handle: WGPUCommandEncoder,
}

impl<'a> CommandEncoder<'a> {
    pub fn begin_compute_pass(&self) -> ComputePassEncoder {
        unsafe {
            ComputePassEncoder {
                handle: wgpuCommandEncoderBeginComputePass(self.handle, &zeroed())
                    .assert_not_null(),
            }
        }
    }

    pub fn copy_buffer_to_buffer(&self, src: &DeviceBuffer, dst: &DeviceBuffer, size: usize) {
        unsafe {
            wgpuCommandEncoderCopyBufferToBuffer(
                self.handle,
                src.handle,
                0,
                dst.handle,
                0,
                size as _,
            );
        }
    }

    pub fn finish(self) -> Result<CommandBuffer> {
        ErrorScope::new(self.device, "command buffer finish failed").execute(|| unsafe {
            CommandBuffer {
                handle: wgpuCommandEncoderFinish(self.handle, &zeroed()).assert_not_null(),
            }
        })
    }
}

impl Drop for CommandEncoder<'_> {
    fn drop(&mut self) {
        unsafe {
            wgpuCommandEncoderRelease(self.handle);
        }
    }
}

pub struct ComputePassEncoder {
    handle: WGPUComputePassEncoder,
}

impl ComputePassEncoder {
    pub fn set_pipeline(&self, pipeline: &ComputePipeline) {
        unsafe {
            wgpuComputePassEncoderSetPipeline(self.handle, pipeline.handle);
        }
    }

    pub fn set_bind_group(&self, index: u32, group: &BindGroup) {
        unsafe {
            wgpuComputePassEncoderSetBindGroup(self.handle, index, group.handle, 0, [].as_ptr());
        }
    }

    pub fn dispatch(&self, x: u32, y: u32, z: u32) {
        unsafe {
            wgpuComputePassEncoderDispatchWorkgroups(self.handle, x, y, z);
        }
    }
}

impl Drop for ComputePassEncoder {
    fn drop(&mut self) {
        unsafe {
            wgpuComputePassEncoderEnd(self.handle);
            wgpuComputePassEncoderRelease(self.handle);
        }
    }
}

pub struct CommandBuffer {
    handle: WGPUCommandBuffer,
}

impl CommandBuffer {}

impl Drop for CommandBuffer {
    fn drop(&mut self) {
        unsafe {
            wgpuCommandBufferRelease(self.handle);
        }
    }
}

trait PointerExt {
    fn assert_not_null(self) -> Self;
}

impl<T> PointerExt for *const T {
    fn assert_not_null(self) -> Self {
        if self.is_null() {
            panic!("null pointer")
        } else {
            self
        }
    }
}

impl<T> PointerExt for *mut T {
    fn assert_not_null(self) -> Self {
        if self.is_null() {
            panic!("null pointer")
        } else {
            self
        }
    }
}

struct ErrorScope<'a> {
    device: &'a Device<'a>,
    message: &'a str,
}

impl<'a> ErrorScope<'a> {
    fn new(device: &'a Device, message: &'a str) -> Self {
        ErrorScope { device, message }
    }

    fn execute<T>(&self, block: impl FnOnce() -> T) -> Result<T> {
        let filters = [
            WGPUErrorFilter_WGPUErrorFilter_Validation,
            WGPUErrorFilter_WGPUErrorFilter_Internal,
        ];

        for &filter in &filters {
            unsafe {
                wgpuDevicePushErrorScope(self.device.handle, filter);
            }
        }

        struct ErrorCapture {
            callback_fired: usize,
            error_message: Option<String>,
        }

        unsafe extern "C" fn callback(
            _status: WGPUPopErrorScopeStatus,
            error_type: WGPUErrorType,
            message: WGPUStringView,
            userdata1: *mut c_void,
            _userdata2: *mut c_void,
        ) {
            let capture = unsafe { &mut *(userdata1 as *mut ErrorCapture) };
            capture.callback_fired += 1;

            if error_type == WGPUErrorType_WGPUErrorType_NoError {
                return;
            }

            if !message.data.is_null() {
                let slice = std::slice::from_raw_parts(message.data as *const u8, message.length);
                let message_str = String::from_utf8_lossy(slice);
                eprintln!("{message_str}");

                if let Some(ref mut existing) = capture.error_message {
                    existing.push_str(": ");
                    existing.push_str(&message_str);
                } else {
                    capture.error_message = Some(message_str.into_owned());
                }
            } else if capture.error_message.is_none() {
                capture.error_message = Some("Unknown error".to_owned());
            }
        }

        let result = block();
        let mut capture = ErrorCapture {
            callback_fired: 0,
            error_message: None,
        };

        for _ in 0..filters.len() {
            let callback_info = WGPUPopErrorScopeCallbackInfo {
                nextInChain: null_mut(),
                mode: WGPUCallbackMode_WGPUCallbackMode_AllowProcessEvents,
                callback: Some(callback),
                userdata1: &mut capture as *mut _ as *mut c_void,
                userdata2: null_mut(),
            };

            unsafe {
                wgpuDevicePopErrorScope(self.device.handle, callback_info);
            }
        }

        while capture.callback_fired < filters.len() {
            self.device._instance.process_events();
        }

        if let Some(err) = capture.error_message {
            Err(eyre!(format!("{}\n{err}", self.message)))
        } else {
            Ok(result)
        }
    }
}

unsafe extern "C" fn default_error_callback(
    _device: *const *mut WGPUDeviceImpl,
    error_type: WGPUErrorType,
    message: WGPUStringView,
    _userdata1: *mut c_void,
    _userdata2: *mut c_void,
) {
    if !message.data.is_null() {
        let slice = std::slice::from_raw_parts(message.data as *const u8, message.length);
        let message_str = String::from_utf8_lossy(slice);
        eprintln!("{message_str}");
    }

    #[allow(non_upper_case_globals)]
    match error_type {
        WGPUErrorType_WGPUErrorType_Validation => {
            panic!("validation error");
        }
        WGPUErrorType_WGPUErrorType_OutOfMemory => {
            panic!("out of memory");
        }
        WGPUErrorType_WGPUErrorType_Internal => {
            panic!("internal error");
        }
        WGPUErrorType_WGPUErrorType_Unknown => {
            panic!("an unknown error occurred");
        }
        _ => {}
    }
}
