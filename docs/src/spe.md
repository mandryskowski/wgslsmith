# Skeletal Program Enumeration (SPE)

Skeletal Program Enumeration (SPE) is a fuzzing technique that takes a skeleton shader and enumerates valid variable assignments for specific "holes" within it. 

```sh
./wgslsmith spe --help
```

The `spe` command has three main subcommands:

- `process-dir <DIR>`: Scans a directory for WGSL shaders, treating each as a skeleton. It automatically enumerates variants and tests them against the configured WebGPU implementations.
- `enumerate <SHADER>`: Generates and prints (or executes) all valid enumerations for a single WGSL shader. Useful for testing a single skeleton.
- `fuse <DIR>`: Iterates over a directory of shaders, fusing them together (using the [fuser](../fuse.md)) to create larger, more complex skeletons, and then tests their permutations.

When running `process-dir` or `fuse`, `wgslsmith` will execute the enumerated shaders and automatically save any crashes or mismatches it finds, similar to the standard `fuzz` command. You can disable this using `--no-file-log`. If you wish to re-use an existing directory, use `--append-dir`.
