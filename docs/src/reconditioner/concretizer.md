# Concretizer

The WGSL specification allows WGSL compilers to throw a validation error for certain invalid const-expressions. For example, this shader:
```rust
@compute @workgroup_size(1)
fn main() {
    _ = 5i / 0i;
}
```
results in the following error when compiled with tint:
```
Error while parsing WGSL: :4:9 error: integer division by zero is invalid
    _ = 5i / 0i;
        ^^^^^^^
```

This is actually a big problem for shaders produced by our random WGSL generator. There are many cases where the specification allows validation errors, and since our generator produces large shaders with many deeply nested expressions, it is likely that we will hit at least one of them. Without the concretizer, less than 1% of generated shaders pass validation, which is bad since this essentially only tests the frontend of tint/naga and leaves downstream compilers untested.

The concretizer detects expressions that would fail validation and replaces them with a safe default. For example, for the shader given above, it will output

```rust
@compute @workgroup_size(1u)
fn main() {
    _ = 5i;
}
```

The `recondition` command calls the concretizer before performing the reconditioning. This way, after reconditioning, we should always get syntactically and semantically valid shaders that pass validation. However, you can apply only the concretization using `concretize`.
```sh
./wgslsmith recondition test.wgsl # This will concretize before reconditioning
./wgslsmith concretize test.wgsl # This will only concretize
```

We do not concretize _every_ expression that would fail validation, and limit it to the cases where tint/naga actually throw an error. We do this hoping that it will allow us to find more bugs. This also means that as tint/naga improve their validators, our concretizer might fail to protect us from validation errors, which would again cause the problem above.

Therefore, we must keep maintaining the concretizer to ensure most of the shaders pass validation. We actually test for this in our CI/CD, and if at least 25/100 generated shaders fail validation, we consider it a fail. You can track the status of this using the badge at the top of `README.md`.