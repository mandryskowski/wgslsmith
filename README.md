# wgslsmith-spe

[![CI](https://github.com/mandryskowski/wgslsmith/actions/workflows/ci.yml/badge.svg)](https://github.com/mandryskowski/wgslsmith-spe/actions/workflows/ci.yml)
[![Valid Shaders](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mandryskowski/wgslsmith-spe/badges/validation-percentage.json)](https://github.com/mandryskowski/wgslsmith-spe/actions/workflows/ci.yml)
[![](https://img.shields.io/badge/docs-wgslsmith--spe.github.io-orange)](https://wgslsmith-spe.github.io)

`wgslsmith-spe` provides a set of tools for randomized testing of [WGSL](https://www.w3.org/TR/WGSL/) shader compilers, including: 

- a random WGSL shader generator
- a native WGSL harness
- support for test case reduction with Perses
- a Skeletal Program Enumerator and a shader fuser.

The supported WebGPU implementations are [Dawn (Chrome)](https://dawn.googlesource.com/dawn) and [wgpu (Firefox)](https://github.com/gfx-rs/wgpu).

## Authors

`wgslsmith` was developed for [@hasali19](https://github.com/hasali19)'s final year project at Imperial College London, and went on to win a prize. The project report is available [here](https://drive.google.com/file/d/1qDcGQndpl5onKN2UA4CFStDJQBRfpKIm/view?usp=sharing).

`wgslsmith-spe` is a fork for [@mandryskowski](https://github.com/mandryskowski)'s final year project at Imperial College London.

## Requirements

- [Rust](https://rustup.rs/) - Latest stable toolchain
- [Python](https://www.python.org/) - Any recent version (required by `build.py`)

A more complete list of requirements is available [here](https://wgslsmith-spe.github.io/building/index.html).

## Building

If you just want to try this out, grab the latest CI/CD release. File an issue if this doesn't work.

### Full build
Clone with `--recursive`.

Once you manage to set up all dependencies correctly (see [docs](https://wgslsmith-spe.github.io/building/index.html)), simply run:
```sh
./build.py
```

Compiling Dawn takes about 30 minutes. Save time by using the pre-built Dawn from [dawn-build](https://github.com/mandryskowski/dawn-build).

```sh
export DAWN_BUILD_DIR=/path/to/unzipped/dawn-build
./build.py
```

You can also compile with AddressSanitizer (ASan) and UndefinedBehaviorSanitizer (UBSan):
```sh
./build.py --asan --ubsan
```

### Without harness/reducer
Compilation is very simple if you don't need the WGSLsmith harness/reducer as Dawn and wgpu are not necessary.

```sh
./build.py --no-harness --no-reducer
```


## Usage

All the tools can be used through the `wgslsmith` command:

```sh
# Do some fuzzing
./wgslsmith fuzz
# Recondition a shader
./wgslsmith recondition /path/to/shader.wgsl
# Reduce a crash
./wgslsmith reduce crash path/to/shader.wgsl --config wgpu:dx12:9348 --regex '...'
# Run a shader
./wgslsmith run path/to/shader.wgsl
```

Some options can be configured through a config file. Run `wgslsmith config` to open the default config file in a text editor. You can also specify a custom config file with the `--config-file` option.

```toml
[harness]
errors = ["nvalid"]

[dawn]
enabled_flags = ["use_dxc", "enable_tint_ir_validation_asserts"]

[reducer.perses]
# You need this if you want to reduce with perses
jar = "/path/to/perses_deploy.jar"
```

To use perses for reduction, grab and build it from [https://github.com/mandryskowski/perses](https://github.com/mandryskowski/perses), then add it to the config as above.

## Development

[Insta](https://github.com/mitsuhiko/insta) is used for snapshot testing the parser.

Install the tool with `cargo install cargo-insta` and use `cargo insta test -p parser` to run the parser tests.
