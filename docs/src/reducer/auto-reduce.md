# Auto-Reduce

`wgslsmith` includes an `auto-reduce` command that simplifies the process of reducing crashes and mismatches found during fuzzing.

```sh
./wgslsmith auto-reduce path/to/fuzzer/out/dir
```

The tool recursively scans the provided directory for fuzzer outputs (directories containing `info.json`, `shader.wgsl`, etc.). For each output, it will:
1. Attempt to reproduce the crash or mismatch live.
2. If it's a crash, prompt you for a regex to match the error output (you can leave it blank to skip).
3. Find the configuration that reproduces the bug.
4. Automatically invoke the reducer (e.g., Perses or C-Reduce) with the correct arguments.

You can also filter the reduction by bug type using the `--filter` option:
```sh
./wgslsmith auto-reduce out/ --filter crash
```
