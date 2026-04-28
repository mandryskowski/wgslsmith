var<private> a : i32;

@compute @workgroup_size(1u)
fn main() {
    a = 1;
    // WGSLSMITH_CONTEXT_MARKER_456
    a = 2;
}