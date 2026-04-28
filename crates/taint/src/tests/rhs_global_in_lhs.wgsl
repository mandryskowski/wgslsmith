var<private> a_456 : i32;
var<private> b_456 : i32;

@compute @workgroup_size(1u)
fn main() {
    a_456 = 1;
    // WGSLSMITH_CONTEXT_MARKER_456
    var x_456 = a_456;
    var y_456 = b_456;
}