# Generator

<!-- toc -->

The program generator is able to randomly generate WGSL programs using a range of language features.

```sh
./wgslsmith gen # Generate a shader
./wgslsmith gen --help # Show help text
```

Note that programs produced by the generator may not always compile (despite being syntactically valid and well-typed). This is because some WGSL compilers implement additional validation such as rejecting obvious infinite loops. wgslsmith uses a technique called reconditioning (see [here](../reconditioner/index.md)) to guarantee validity. You can recondition shaders by passing `--recondition` to the generator, or by invoking the reconditioner separately on the generated shader which allows more control over its behaviour.

The generator has various options to control the generation process. See the help text for a full list.

## Stage

By default, the generator outputs compute shaders. Use the `--stage` option to get vertex or fragment shaders. This of course affects the entrypoint (its return type), but also importantly the builtins that the generator can produce. For example, derivative functions such as `dpdx` can only be called from fragment shaders.

## WGSL extensions

Some features of WGSL are hidden behind extensions. The generator currently supports the `f16` and `subgroups` extensions.

By default, the generator uses no extensions, because their availability depends on the device you are testing. You can enable them using the `--gen-ext` option, for example `--gen-ext subgroups`.

## Floating-point builtins for crash testing

By default, the generator is quite conservative and avoids generating builtins that are likely to produce floating-point errors. This is to ensure that you don't get flooded with false positives when doing differential testing.

For crash testing, you may allow the generation of these builtins using `--unstable-float`. The output still needs reconditioning to avoid dynamic errors and concretisation to avoid validation errors. You can also pass `--unstable-float` to [reconditioner](../reconditioner/index.md) to get lighter reconditioning for crash testing.

## Context-aware generation

You can seed the generator with symbols (e.g. global variables, functions, structs) of an existing test case using the `--context` option. The generator will still generate its own symbols if this option is passed. It may be helpful to use this option with test cases from tint/naga test suites, as the generator may not produce some types of symbols, like nested arrays.

## Pointers

Pointers are currently supported as an opt-in feature (since the reconditioner may reject some shaders with invalid pointer operations). To enable them, use the `--enable-pointers` flag. If reconditioning (with `--recondition`), you can also pass `--skip-pointer-checks` to stop it from erroring if the program contains possible invalid pointer operations.
