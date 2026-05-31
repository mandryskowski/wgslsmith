# Daemon Mode

On Windows, initializing WebGPU implementations (like Dawn or wgpu) can take a significant amount of time, sometimes adding seconds of overhead to every execution. This drastically slows down fuzzing and reduction.

To avoid this overhead, the harness can be run in _daemon mode_. The daemon persists the WebGPU instance and device state in the background and accepts execution requests over TCP.

## Using the Daemon

To route execution requests to the daemon, simply pass the `--use-daemon` flag to the `run`, `fuzz`, `reduce`, or `spe` commands.

```sh
./wgslsmith run shader.wgsl --use-daemon
```

If the daemon is not already running, `wgslsmith` will automatically spawn it invisibly in the background. The daemon will automatically shut down after a period of inactivity (default 5 minutes).

If the compilation times out, the daemon aborts, and will start again when the next shader runs.

The daemon logs its stdout and stderr to the tmp directory. It might be worth reviewing these logs periodically to look for anything interesting. Useful commands here are `grep` to look for specific messages or `tail` to see the messages that the daemon produced before dying.

## Manual Management

You can also start the daemon manually if you want to observe its logs:

```sh
./wgslsmith harness daemon
```

By default, the daemon listens on `127.0.0.1:9000`. You can override the port being used with the `--daemon-port` argument when invoking the client commands.
