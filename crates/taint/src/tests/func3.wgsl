struct Struct_456 {
    a: vec3i
}

var<private> s_456: Struct_456;

@compute @workgroup_size(1u)
fn main() {
    s_456.a.y = 1i;

    // WGSLSMITH_CONTEXT_MARKER_456
    
}

fn foo_456() {
    var b = s_456.a.y;
    var c = s_456.a.x;
}