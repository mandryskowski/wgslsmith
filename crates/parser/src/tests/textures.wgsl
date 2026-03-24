// @group(0) @binding(0) var t1: texture_1d<f32>;
// @group(0) @binding(1) var t2: texture_2d<f32>;
// @group(0) @binding(2) var t3: texture_2d_array<f32>;
// @group(0) @binding(3) var t4: texture_3d<f32>;
// @group(0) @binding(4) var t5: texture_cube<f32>;
// @group(0) @binding(5) var t6: texture_cube_array<f32>;
// @group(0) @binding(6) var t7: texture_multisampled_2d<f32>;
// @group(0) @binding(7) var t8: texture_storage_1d<rgba8unorm, write>;
// @group(0) @binding(8) var t9: texture_storage_2d<r32float, read>;
// @group(0) @binding(9) var t10: texture_storage_2d_array<rg32uint, read_write>;
// @group(0) @binding(10) var t11: texture_storage_3d<rgba16sint, write>;
// @group(0) @binding(11) var t12: texture_depth_2d;
// @group(0) @binding(12) var t13: texture_depth_2d_array;
// @group(0) @binding(13) var t14: texture_depth_cube;
// @group(0) @binding(14) var t15: texture_depth_cube_array;
// @group(0) @binding(15) var t16: texture_depth_multisampled_2d;
// @group(0) @binding(16) var t17: texture_external;

// @group(1) @binding(0) var s1: sampler;
// @group(1) @binding(1) var s2: sampler_comparison;

// @compute @workgroup_size(1)
// fn main() {}
// --- Setup Resources for Testing ---
@group(0) @binding(0) var t_1d: texture_1d<f32>;
@group(0) @binding(1) var t_2d: texture_2d<f32>;
@group(0) @binding(2) var t_2d_array: texture_2d_array<f32>;
@group(0) @binding(3) var t_3d: texture_3d<f32>;
@group(0) @binding(4) var t_cube: texture_cube<f32>;
@group(0) @binding(5) var t_cube_array: texture_cube_array<f32>;
@group(0) @binding(6) var t_multi: texture_multisampled_2d<f32>;
@group(0) @binding(7) var t_depth: texture_depth_2d;
@group(0) @binding(8) var t_depth_array: texture_depth_2d_array;
@group(0) @binding(9) var t_depth_cube: texture_depth_cube;
@group(0) @binding(10) var t_depth_cube_array: texture_depth_cube_array;
@group(0) @binding(11) var t_depth_multi: texture_depth_multisampled_2d;
@group(0) @binding(12) var t_storage: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(13) var t_external: texture_external;

@group(0) @binding(14) var s: sampler;
@group(0) @binding(15) var s_cmp: sampler_comparison;

fn test_textures() {
    // Common coordinates
    let c1 = 0.5;
    let c2 = vec2<f32>(0.5);
    let c3 = vec3<f32>(0.5);
    let c2_i = vec2<i32>(0);
    let c3_i = vec3<i32>(0);
    
    // --- 17.7.1. textureDimensions ---
    // 1D
    let dim_1d: u32 = textureDimensions(t_1d);
    let dim_1d_lod: u32 = textureDimensions(t_1d, 0);
    // 2D
    let dim_2d: vec2<u32> = textureDimensions(t_2d);
    let dim_2d_lod: vec2<u32> = textureDimensions(t_2d, 0);
    // 3D
    let dim_3d: vec3<u32> = textureDimensions(t_3d);
    let dim_3d_lod: vec3<u32> = textureDimensions(t_3d, 0);
    // Cube
    let dim_cube: vec2<u32> = textureDimensions(t_cube);
    // Multisampled
    let dim_multi: vec2<u32> = textureDimensions(t_multi);
    // Storage
    let dim_storage: vec2<u32> = textureDimensions(t_storage);

    // --- 17.7.2. textureGather ---
    // Component 0, 2D
    // let gather_2d: vec4<f32> = textureGather(0, t_2d, s, c2);
    // Component 1, 2D Array with offset
    // let gather_2d_arr: vec4<f32> = textureGather(1, t_2d_array, s, c2, 0, vec2<i32>(1, 0));
    // Cube
    // let gather_cube: vec4<f32> = textureGather(0, t_cube, s, c3);
    // Depth (no component parameter)
    let gather_depth: vec4<f32> = textureGather(t_depth, s, c2);

    // --- 17.7.3. textureGatherCompare ---
    // Depth 2D
    let gather_cmp_2d: vec4<f32> = textureGatherCompare(t_depth, s_cmp, c2, 0.5);
    // Depth 2D Array with offset
    let gather_cmp_arr: vec4<f32> = textureGatherCompare(t_depth_array, s_cmp, c2, 0, 0.5, vec2<i32>(1, 0));
    // Depth Cube
    let gather_cmp_cube: vec4<f32> = textureGatherCompare(t_depth_cube, s_cmp, c3, 0.5);

    // --- 17.7.4. textureLoad ---
    // 1D (coords, level)
    let load_1d: vec4<f32> = textureLoad(t_1d, 0, 0);
    // 2D (coords, level)
    let load_2d: vec4<f32> = textureLoad(t_2d, c2_i, 0);
    // 2D Array (coords, array_index, level)
    let load_2d_arr: vec4<f32> = textureLoad(t_2d_array, c2_i, 0, 0);
    // 3D
    let load_3d: vec4<f32> = textureLoad(t_3d, c3_i, 0);
    // Multisampled (coords, sample_index)
    let load_multi: vec4<f32> = textureLoad(t_multi, c2_i, 0);
    // Depth
    let load_depth: f32 = textureLoad(t_depth, c2_i, 0);
    // External
    let load_ext: vec4<f32> = textureLoad(t_external, c2_i);
    // Storage
    let load_storage: vec4<f32> = textureLoad(t_storage, c2_i);

    // --- 17.7.5. textureNumLayers ---
    let layers_2d: u32 = textureNumLayers(t_2d_array);
    let layers_cube: u32 = textureNumLayers(t_cube_array);
    let layers_depth: u32 = textureNumLayers(t_depth_array);

    // --- 17.7.6. textureNumLevels ---
    let levels_1d: u32 = textureNumLevels(t_1d);
    let levels_2d: u32 = textureNumLevels(t_2d);
    let levels_3d: u32 = textureNumLevels(t_3d);
    let levels_cube: u32 = textureNumLevels(t_cube);

    // --- 17.7.7. textureNumSamples ---
    let samples_ms: u32 = textureNumSamples(t_multi);
    let samples_depth_ms: u32 = textureNumSamples(t_depth_multi);

    // --- 17.7.8. textureSample ---
    // 1D
    let sample_1d: vec4<f32> = textureSample(t_1d, s, c1);
    // 2D
    let sample_2d: vec4<f32> = textureSample(t_2d, s, c2);
    // 2D with offset
    let sample_2d_off: vec4<f32> = textureSample(t_2d, s, c2, vec2<i32>(1, 0));
    // 2D Array
    let sample_2d_arr: vec4<f32> = textureSample(t_2d_array, s, c2, 0);
    // Cube
    let sample_cube: vec4<f32> = textureSample(t_cube, s, c3);
    // Depth
    let sample_depth: f32 = textureSample(t_depth, s, c2);

    // --- 17.7.9. textureSampleBias ---
    // 2D
    let sample_bias_2d: vec4<f32> = textureSampleBias(t_2d, s, c2, 1.0);
    // 2D with offset
    let sample_bias_off: vec4<f32> = textureSampleBias(t_2d, s, c2, 1.0, vec2<i32>(1, 0));
    // 3D
    let sample_bias_3d: vec4<f32> = textureSampleBias(t_3d, s, c3, 1.0);
    // Cube
    let sample_bias_cube: vec4<f32> = textureSampleBias(t_cube, s, c3, 1.0);

    // --- 17.7.10. textureSampleCompare ---
    // Depth 2D
    let sample_cmp_2d: f32 = textureSampleCompare(t_depth, s_cmp, c2, 0.5);
    // Depth 2D with offset
    let sample_cmp_off: f32 = textureSampleCompare(t_depth, s_cmp, c2, 0.5, vec2<i32>(1, 0));
    // Depth Array
    let sample_cmp_arr: f32 = textureSampleCompare(t_depth_array, s_cmp, c2, 0, 0.5);
    // Depth Cube
    let sample_cmp_cube: f32 = textureSampleCompare(t_depth_cube, s_cmp, c3, 0.5);

    // --- 17.7.11. textureSampleCompareLevel ---
    // Exact same signatures as textureSampleCompare, but explicit LOD 0
    let sample_cmplvl_2d: f32 = textureSampleCompareLevel(t_depth, s_cmp, c2, 0.5);
    let sample_cmplvl_cube: f32 = textureSampleCompareLevel(t_depth_cube, s_cmp, c3, 0.5);

    // --- 17.7.12. textureSampleGrad ---
    let ddx2 = vec2<f32>(0.1);
    let ddy2 = vec2<f32>(0.1);
    let ddx3 = vec3<f32>(0.1);
    let ddy3 = vec3<f32>(0.1);
    
    // 2D
    let sample_grad_2d: vec4<f32> = textureSampleGrad(t_2d, s, c2, ddx2, ddy2);
    // 2D with offset
    let sample_grad_off: vec4<f32> = textureSampleGrad(t_2d, s, c2, ddx2, ddy2, vec2<i32>(1, 0));
    // 3D
    let sample_grad_3d: vec4<f32> = textureSampleGrad(t_3d, s, c3, ddx3, ddy3);
    // Cube Array
    let sample_grad_cube_arr: vec4<f32> = textureSampleGrad(t_cube_array, s, c3, 0, ddx3, ddy3);

    // --- 17.7.13. textureSampleLevel ---
    // 1D
    let sample_lvl_1d: vec4<f32> = textureSampleLevel(t_1d, s, c1, 0.0);
    // 2D
    let sample_lvl_2d: vec4<f32> = textureSampleLevel(t_2d, s, c2, 0.0);
    // Depth (Level is integer here for Depth textures)
    let sample_lvl_depth: f32 = textureSampleLevel(t_depth, s, c2, 0);
    // Depth Array
    let sample_lvl_depth_arr: f32 = textureSampleLevel(t_depth_array, s, c2, 0, 0);

    // --- 17.7.14. textureSampleBaseClampToEdge ---
    // 2D
    let sample_clamp_2d: vec4<f32> = textureSampleBaseClampToEdge(t_2d, s, c2);
    // External
    let sample_clamp_ext: vec4<f32> = textureSampleBaseClampToEdge(t_external, s, c2);

    // --- 17.7.15. textureStore ---
    let color = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    textureStore(t_storage, c2_i, color);
}

@compute @workgroup_size(1)
fn main() {
    test_textures();
}