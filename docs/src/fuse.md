# Shader Fuser

The fuser combines two WGSL shaders into a single larger WGSL program. This is useful if you want to obtain one more complex shaders from smaller ones.

```sh
./wgslsmith fuse path/to/a.wgsl path/to/b.wgsl
```

The tool automatically renames some to avoid name collisions. It typically appends a random hash to the names of structs and global variables. This way, it is straightforward to fuse more than 2 shaders together - you just need to fuse the first two shaders, then the output of that with the third shader, and so on.

The fuser merges entrypoints from A and B. Return statements are stripped from A and the statement from B are appended to the end of A.

After you merge some shaders, it might be worth running [SPE](./spe.md) on the result to introduce data dependencies between the original shaders.