# Execution result

The harness can produce two types of errors:

- If the actual shader execution failed, this will manifest as a panic with exit code `101`.
- If the shader was successfully executed for all configurations but the outputs differ, the program will exit with code `1`.

Otherwise, the program exits normally with code `0`, and the execution is considered benign.

## Stdout/stderr-based failures

Sometimes, you might want to consider an execution a crash if a certain message appears in stdout/stderr. For example, when running with Metal Shader Validation, a shader reading out of bounds could produce an `Invalid load at offset` message, but will not cause a crash. You can add regexes for messages in stderr using the config file:

```toml
[harness]
errors = ["Invalid load"]
```

If the regex matches the stdout/stderr of the execution, the harness returns `101`. 

## Reduction
Test case reduction tools such as [c-reduce](https://embed.cs.utah.edu/creduce/) typically take an _interestingness_ test as input, which returns `0` for a useful test case or `1` if the test case should be discarded.


Normally when using this with a reduction tool to find miscompilations, you will want to discard the shader if the harness returns `0` or `101`, since execution failure means that the reduction process probably produced an invalid program. Only the exits with `1` are likely to be interesting.
