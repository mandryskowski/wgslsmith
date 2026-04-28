var<private> a_123: i32;

@compute @workgroup_size(1u)
fn main() {
    var x_123 = 1;
    // WGSLSMITH_CONTEXT_MARKER_456
    var y_456 = x_123;
    a_123 = y_456;
}