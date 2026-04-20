#include <iostream>
#include <memory>
#include <vector>

#include <dawn/dawn_proc.h>
#include <dawn/webgpu.h>
#include <dawn/webgpu_cpp.h>
#include <dawn/native/DawnNative.h>

static void DeviceLogCallback(WGPULoggingType type, WGPUStringView message, void* userdata, void* userdata2) {
    const char* typeName = "Info";
    switch (type) {
        case WGPULoggingType_Verbose: typeName = "Verbose"; break;
        case WGPULoggingType_Info:    typeName = "Info"; break;
        case WGPULoggingType_Warning: typeName = "Warning"; break;
        case WGPULoggingType_Error:   typeName = "Error"; break;
        default: break;
    }

    if (message.length == SIZE_MAX) {
        fprintf(stderr, "[Dawn %s] %s\n", typeName, message.data);
    } else {
        fprintf(stderr, "[Dawn %s] %.*s\n", typeName, (int)message.length, message.data);
    }

    fflush(stderr);
}

extern "C" dawn::native::Instance* new_instance() {
    // Initialize WebGPU proc table
    dawnProcSetProcs(&dawn::native::GetProcs());

    auto instance = new dawn::native::Instance;

    // This makes things slow
    // instance->SetBackendValidationLevel(dawn::native::BackendValidationLevel::Full);

    return instance;
}

extern "C" void instance_process_events(dawn::native::Instance* instance) {
    if (instance) {
        wgpuInstanceProcessEvents(instance->Get());
    }
}

extern "C" void delete_instance(dawn::native::Instance* instance) {
    delete instance;
}

extern "C" void enumerate_adapters(
    const dawn::native::Instance* instance,
    void(*callback)(const WGPUAdapterInfo*, void*),
    void* userdata
) {
    if (callback == nullptr) return;

    WGPURequestAdapterOptions options = {};
    auto native_adapters = instance->EnumerateAdapters(&options);

    for (auto& native_adapter : native_adapters) {
        WGPUAdapter adapterHandle = native_adapter.Get();
        WGPUAdapterInfo info = {};

        wgpuAdapterGetInfo(adapterHandle, &info);

        callback(&info, userdata);
    }
}

extern "C" WGPUDevice create_device(
    const dawn::native::Instance* instance,
    WGPUBackendType backendType,
    uint32_t deviceID,
    WGPUUncapturedErrorCallback errorCallback,
    void* errorUserdata,
    const WGPUFeatureName* requiredFeatures,
    size_t requiredFeatureCount,
    const char* const* enabledToggles,
    size_t enabledToggleCount,
    const char* const* disabledToggles,
    size_t disabledToggleCount
) {
    WGPURequestAdapterOptions options = {};
    auto native_adapters = instance->EnumerateAdapters(&options);

    for (auto& native_adapter : native_adapters) {
        WGPUAdapter adapter_handle = native_adapter.Get();

        WGPUAdapterInfo info = {};
        wgpuAdapterGetInfo(adapter_handle, &info);

        if (info.backendType == backendType && info.deviceID == deviceID) {
            std::vector<WGPUFeatureName> supportedFeatures;
            for (size_t i = 0; i < requiredFeatureCount; ++i) {
                if (wgpuAdapterHasFeature(adapter_handle, requiredFeatures[i])) {
                    supportedFeatures.push_back(requiredFeatures[i]);
                } else {
                    fprintf(stderr, "[Dawn Info] Filtering out unsupported feature: %d\n", (int)requiredFeatures[i]);
                }
            }

            WGPUDawnTogglesDescriptor toggles = {};
            toggles.chain.sType = WGPUSType_DawnTogglesDescriptor;
            toggles.enabledToggleCount = enabledToggleCount;
            toggles.enabledToggles = enabledToggles;
            toggles.disabledToggleCount = disabledToggleCount;
            toggles.disabledToggles = disabledToggles;

            WGPUDeviceDescriptor descriptor = {};
            descriptor.nextInChain = reinterpret_cast<WGPUChainedStruct*>(&toggles);

            WGPUUncapturedErrorCallbackInfo errorCallbackInfo = {};
            errorCallbackInfo.callback = errorCallback;
            errorCallbackInfo.userdata1 = errorUserdata;

            descriptor.requiredFeatures = supportedFeatures.data();
            descriptor.requiredFeatureCount = supportedFeatures.size();

            WGPULimits supportedLimits = {};
            wgpuAdapterGetLimits(adapter_handle, &supportedLimits);

            WGPULimits requiredLimits = WGPU_LIMITS_INIT;
            requiredLimits.maxStorageBuffersPerShaderStage = supportedLimits.maxStorageBuffersPerShaderStage;

            descriptor.requiredLimits = &requiredLimits;

            descriptor.uncapturedErrorCallbackInfo = errorCallbackInfo;

            WGPUDevice device = wgpuAdapterCreateDevice(adapter_handle, &descriptor);

            if (device) {
                WGPULoggingCallbackInfo logCallbackInfo = {};
                logCallbackInfo.nextInChain = nullptr;
                logCallbackInfo.callback = DeviceLogCallback;

                wgpuDeviceSetLoggingCallback(device, logCallbackInfo);
            }

            return device;
        }
    }

    return nullptr;
}
