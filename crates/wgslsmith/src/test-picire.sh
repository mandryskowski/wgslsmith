#!/usr/bin/env bash

kill -s USR1 "$WGSLREDUCE_PID"

args=(
    "$WGSLREDUCE_KIND"
    "$1"
    "$WGSLREDUCE_METADATA_PATH"
)

if [[ -n "${WGSLREDUCE_SERVER-}" ]]; then
    args+=("--server" "$WGSLREDUCE_SERVER")
fi

if [[ -n "${WGSLREDUCE_USE_DAEMON-}" ]]; then
    args+=("--use-daemon")
    if [[ -n "${WGSLREDUCE_DAEMON_PORT-}" ]]; then
        args+=("--daemon-port" "$WGSLREDUCE_DAEMON_PORT")
    fi
fi

if [[ -n "${WGSLREDUCE_CONFIGS-}" ]]; then
    for conf in $WGSLREDUCE_CONFIGS; do
        args+=("--config" "$conf")
    done
fi

if [[ "$WGSLREDUCE_KIND" == "crash" ]]; then
    args+=("--regex" "$WGSLREDUCE_REGEX")

    if [[ -z "${WGSLREDUCE_CONFIGS-}" ]]; then
        args+=(
            "--compiler" "$WGSLREDUCE_COMPILER"
            "--backend" "$WGSLREDUCE_BACKEND"
        )
    fi

    if [[ ! -n "${WGSLREDUCE_RECONDITION}" ]]; then
        args+=("--no-recondition")
    fi
fi

if [[ -n "${WGSLREDUCE_ATTEMPTS-}" ]]; then
    args+=("--attempts" "$WGSLREDUCE_ATTEMPTS")
fi

[WGSLSMITH] test -q "${args[@]}" >/dev/null 2>&1
