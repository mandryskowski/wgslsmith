struct Inner {
    a: array<vec3i, 1>,
}

struct S {
    a: Inner
}

@group(0) @binding(0)
var<storage, read_write> s : S;

@compute @workgroup_size(1)
fn main() {
    s.a.a[0] = vec3i(1);
    // WGSLSMITH_CONTEXT_MARKER_456
    s.a.a[0] += vec3i(1);
}