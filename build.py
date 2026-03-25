#!/usr/bin/env python3

import argparse
import os
import shutil
import subprocess

from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("task", nargs="?", default="wgslsmith")
    parser.add_argument("--target")
    parser.add_argument("--install-prefix")
    parser.add_argument("--no-reducer", action="store_true")
    parser.add_argument("--no-harness", action="store_true")
    parser.add_argument("--dawn-path", default="external/dawn")
    parser.add_argument("--asan", action="store_true", help="Compile with AddressSanitizer (ASan)")
    parser.add_argument("--ubsan", action="store_true", help="Compile with UndefinedBehaviorSanitizer (UBSan)")
    return parser.parse_args()


args = parse_args()

if args.asan or args.ubsan:
    os.environ["CC"] = os.environ.get("CC", "clang")
    os.environ["CXX"] = os.environ.get("CXX", "clang++")


def get_cargo_host_target():
    output = subprocess.check_output(["cargo", "-Vv"]).decode()
    for line in output.splitlines():
        if line.startswith("host:"):
            return line.split(":")[1].strip()


root_dir = Path(os.path.realpath(__file__)).parent
host_target = get_cargo_host_target()
build_target = args.target if args.target is not None else host_target
is_cross = args.target is not None and host_target != args.target

dawn_src_dir = Path(args.dawn_path)
dawn_build_dir = Path(f"build/dawn/{build_target}")

def get_commit(git_dir):
    output = subprocess.check_output(["git", "--git-dir", git_dir, "rev-parse", "HEAD"])
    return output.decode().strip()


def read_gclient_sync_hash():
    path = Path("build/dawn/gclient_sync_hash")
    if path.exists():
        return path.read_text().strip()


def write_gclient_sync_hash(hash):
    path = Path("build/dawn/gclient_sync_hash")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(hash)


def gen_cmake_build(src_dir: Path, build_dir: Path, args=[], env={}):
    build_dir.mkdir(parents=True, exist_ok=True)

    config_marker = build_dir.joinpath(".cmake_config_args")
    current_config = f"args={args}\nenv={env}"
    
    if build_dir.joinpath("build.ninja").exists() and config_marker.exists():
        if config_marker.read_text() == current_config:
            print(f"> CMake for {build_dir.name} already generated, skipping.")
            return

    cmd = [
        "cmake",
        "-GNinja",
        "-DCMAKE_BUILD_TYPE=Release",
        f'-DCMAKE_ARCHIVE_OUTPUT_DIRECTORY={build_dir.absolute().joinpath("lib")}',
        "-DTINT_BUILD_HLSL_WRITER=ON",
        "-DTINT_BUILD_MSL_WRITER=ON",
        "-DTINT_BUILD_SPV_WRITER=ON",
        *args,
        src_dir.absolute(),
    ]

    cmd_env = os.environ.copy()
    cmd_env.update(env)

    subprocess.run(cmd, cwd=build_dir, env=cmd_env).check_returncode()

    config_marker.write_text(current_config)


def cmake_build(build_dir: Path, targets=[]):
    cmd = ["cmake", "--build", ".", "--target", *targets]
    print(f">> {' '.join(cmd)}")
    subprocess.run(cmd, cwd=build_dir).check_returncode()


def cargo_build(package, target=None, cwd=None, features=[]):
    if target and "android" in target:
        cmd = ["cargo", "ndk", "-t", target, "--platform", "30", "build", "-p", package, "--release"]
    else:
        cmd = ["./cargo", "build", "-p", package, "--release"]
        if target:
            cmd += ["--target", target]
            
    if len(features) > 0:
        cmd += ["--features", ",".join(features)]

    cmd += ["--config", f'env.DAWN_SRC_DIR="{dawn_src_dir}"']

    env = os.environ.copy()

    if args.asan:
        env["DAWN_ASAN"] = "1"
    if args.ubsan:
        env["DAWN_UBSAN"] = "1"

    if args.asan or args.ubsan:
        san_flags = []
        if args.asan:
            san_flags.append("-fsanitize=address")
        if args.ubsan:
            san_flags.append("-fsanitize=undefined")
        
        flags = " ".join(san_flags)
        env["CFLAGS"] = f"{env.get('CFLAGS', '')} {flags}".strip()
        env["CXXFLAGS"] = f"{env.get('CXXFLAGS', '')} {flags}".strip()

    if args.asan or args.ubsan:
        if target and "msvc" in target:
            msvc_rustflags_key = "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS"
            msvc_flags = env.get(msvc_rustflags_key, "")

            clang_bin = f"{os.environ['LLVM_NATIVE_TOOLCHAIN']}/bin/clang"
            resource_dir = subprocess.check_output([clang_bin, "-print-resource-dir"]).decode().strip()
            win_lib_dir = f"{resource_dir}/lib/windows"
            msvc_flags += f" -Lnative={win_lib_dir}"

            if args.asan:
                msvc_flags += " -C link-arg=clang_rt.asan_dynamic-x86_64.lib"
                msvc_flags += " -C link-arg=clang_rt.asan_dynamic_runtime_thunk-x86_64.lib"
                msvc_flags += " -C link-arg=-include:__asan_seh_interceptor"
                msvc_flags += " -C link-arg=-wholearchive:clang_rt.asan_dynamic_runtime_thunk-x86_64.lib"
                msvc_flags += " -C link-arg=/NODEFAULTLIB:stl_asan.lib"
                msvc_flags += " -C link-arg=/NODEFAULTLIB:vcasan.lib"
                # Suppress Visual Studio STL ASan detection since we lack stl_asan.lib
                env["CXXFLAGS"] = f"{env.get('CXXFLAGS', '')} /D_HAS_ASAN=0".strip()
            if args.ubsan:
                msvc_flags += " -C link-arg=clang_rt.ubsan_standalone-x86_64.lib"

            env[msvc_rustflags_key] = msvc_flags.strip()
        elif target and "apple" in target:
            wrapper_path = Path(cwd if cwd else ".").absolute().joinpath("clang++-wrapper")
            if not wrapper_path.exists():
                wrapper_path.write_text("#!/bin/bash\nargs=()\nfor arg in \"$@\"; do\n  if [ \"$arg\" != \"-nodefaultlibs\" ]; then\n    args+=(\"$arg\")\n  fi\ndone\nexec clang++ \"${args[@]}\"\n")
                wrapper_path.chmod(0o755)

            rustflags = env.get("RUSTFLAGS", "")
            rustflags += f" -C linker={wrapper_path}"
            if args.asan:
                rustflags += " -C link-arg=-fsanitize=address"
            if args.ubsan:
                rustflags += " -C link-arg=-fsanitize=undefined"
            rustflags += " -C link-arg=-lc++"
            env["RUSTFLAGS"] = rustflags.strip()
        else:
            # For native (Linux) targets, use clang++ as linker driver
            rustflags = env.get("RUSTFLAGS", "")
            rustflags += " -C linker=clang++"
            if args.asan:
                rustflags += " -C link-arg=-fsanitize=address"
            if args.ubsan:
                rustflags += " -C link-arg=-fsanitize=undefined"
            if target and "android" in target:
                pass
            else:
                rustflags += " -C link-arg=-lstdc++"
            env["RUSTFLAGS"] = rustflags.strip()

    if target and "msvc" in target:
        xwin_dir = os.environ.get("XWIN_CACHE")

        # For some reason bindgen needs these to find math.h (and possibly others)
        includes = [
            f"-I{xwin_dir}/crt/include",
            f"-I{xwin_dir}/sdk/include/ucrt",
            f"-I{xwin_dir}/sdk/include/shared",
            f"-I{xwin_dir}/sdk/include/um",
            f"-I{xwin_dir}/sdk/include/winrt",
        ]

        env["BINDGEN_EXTRA_CLANG_ARGS"] = " ".join(includes)

    print(f">> {' '.join(cmd)}")
    subprocess.run(cmd, cwd=cwd, env=env).check_returncode()

def bootstrap_gclient_config():
    gclient_config = Path(f'{dawn_src_dir}/.gclient')
    gclient_config_tmpl = Path(f'{dawn_src_dir}/scripts/standalone.gclient')
    if not gclient_config.exists():
        shutil.copyfile(gclient_config_tmpl, gclient_config)


def gclient_sync():
    dawn_commit = get_commit(f'{dawn_src_dir}/.git')
    print(f'dawn commit is: {dawn_commit}')
    gclient_sync_hash = read_gclient_sync_hash()
    if gclient_sync_hash != dawn_commit:
        print("> dawn commit has changed, rerunning gclient sync")
        subprocess.run(["gclient", "sync"], cwd=dawn_src_dir).check_returncode()
        write_gclient_sync_hash(dawn_commit)

def dawn_gen_cmake():
    if is_cross and build_target != "x86_64-pc-windows-msvc":
        print(f"cannot build dawn for target '{build_target}' (host={host_target})")
        exit(1)

    cmake_args = []
    if args.asan or args.ubsan:
        san_flags = []
        if args.asan:
            cmake_args.append("-DDAWN_ENABLE_ASAN=ON")
            san_flags.append("-fsanitize=address")
        if args.ubsan:
            cmake_args.append("-DDAWN_ENABLE_UBSAN=ON")
            san_flags.append("-fsanitize=undefined")
        
        link_flags = []

        if is_cross and build_target == "x86_64-pc-windows-msvc":
            # For MSVC cross-compiling, CMake invokes lld-link directly.
            # We must manually pass the sanitizer libraries to the linker.
            clang_bin = f"{os.environ['LLVM_NATIVE_TOOLCHAIN']}/bin/clang"
            resource_dir = subprocess.check_output([clang_bin, "-print-resource-dir"]).decode().strip()
            win_lib_dir = f"{resource_dir}/lib/windows"
            link_flags.append(f"-libpath:{win_lib_dir}")
            
            if args.asan:
                link_flags.extend([
                    "clang_rt.asan_dynamic-x86_64.lib",
                    "clang_rt.asan_dynamic_runtime_thunk-x86_64.lib",
                    "-include:__asan_seh_interceptor",
                    "-wholearchive:clang_rt.asan_dynamic_runtime_thunk-x86_64.lib",
                ])
            if args.ubsan:
                link_flags.append("clang_rt.ubsan_standalone-x86_64.lib")

        # We need to explicitly expose the sanitizer flags to CMake globally
        # to ensure that Abseil is built with the identical flags as Dawn.
        # We pass these via env vars (CFLAGS/CXXFLAGS) rather than
        # -DCMAKE_C_FLAGS, because the latter overrides WinMsvc.cmake's
        # CMAKE_C_FLAGS_INIT which contains the MSVC STL include paths.
        flags = " ".join(san_flags)
        lf = " ".join(link_flags)

    if is_cross and build_target == "x86_64-pc-windows-msvc":
        cmake_args += [
            f"-DLLVM_NATIVE_TOOLCHAIN={os.environ['LLVM_NATIVE_TOOLCHAIN']}",
            f"-DXWIN_CACHE={os.environ['XWIN_CACHE']}",
            f"-DCMAKE_TOOLCHAIN_FILE={Path('cmake/WinMsvc.cmake').absolute()}",
            "-DDAWN_FORCE_SYSTEM_COMPONENT_LOAD=ON",
        ]
        if args.asan or args.ubsan:
            cmake_args.append("-DCMAKE_TRY_COMPILE_CONFIGURATION=Release")

        san_cflags = flags if (args.asan or args.ubsan) else ""
        san_ldflags = lf if (args.asan or args.ubsan) else ""
        env = {
            "CFLAGS": san_cflags,
            "CXXFLAGS": f"-Wno-float-equal {san_cflags}".strip(),
            "LDFLAGS": san_ldflags,
        }

        gen_cmake_build(
            dawn_src_dir,
            dawn_build_dir,
            cmake_args,
            env,
        )
    else:
        # If ASan/UBSan is toggled, we must ensure CMake re-evaluates.
        # But `gen_cmake_build` will just run `cmake -GNinja` which handles re-runs.
        if args.asan or args.ubsan:
            cmake_args += [
                f"-DCMAKE_C_FLAGS={flags}",
                f"-DCMAKE_CXX_FLAGS={flags}",
            ]
        gen_cmake_build(dawn_src_dir, dawn_build_dir, cmake_args)


def build_tint():
    print(f"> building tint (target={build_target})")
    cmake_build(dawn_build_dir, ["tint_cmd_tint_cmd"])


def build_wgslsmith():
    print(f"> building wgslsmith (target={build_target})")
    features = []
    if not args.no_reducer:
        features.append("reducer")
    if not args.no_harness:
        features.append("harness")
    cargo_build("wgslsmith", target=args.target, features=features)


def build_dawn():
    print(f"> building dawn (target={build_target})")
    cmake_build(dawn_build_dir, ["dawn_native", "dawn_proc"])


def build_harness():
    print(f"> building harness (target={build_target})")
    cargo_build("harness", target=args.target)


if args.task not in {"wgslsmith", "harness", "install"}:
    print(f"invalid task: {args.task}")
    exit(1)

print(f"> task: {args.task}")

if args.task == "install":
    prefix = Path(args.install_prefix if args.install_prefix else "/usr/local/bin")

    wgslsmith = Path("target/release/wgslsmith").absolute()
    link = prefix.joinpath("wgslsmith")

    if not wgslsmith.exists():
        print(f"'{wgslsmith}' does not exist, make sure to run './build.py wgslsmith'")
    elif not link.exists():
        print(f"> linking '{link}' to '{wgslsmith}'")
        link.symlink_to(wgslsmith)
    else:
        print(f"'{link}' already exists")

    exit(0)

# CI/CD Detection: skip dawn build if pre-built libraries are provided
use_prebuilt_dawn = "DAWN_BUILD_DIR" in os.environ

tasks = []

if not use_prebuilt_dawn:
    tasks += [
        bootstrap_gclient_config,
        gclient_sync,
        dawn_gen_cmake,
    ]
else:
    print(f"> Using prebuilt Dawn from: {os.environ['DAWN_BUILD_DIR']}")

if args.task == "wgslsmith":
    if not args.no_reducer and not use_prebuilt_dawn:
        tasks += [build_tint]
    if not args.no_harness and not use_prebuilt_dawn:
        tasks += [build_dawn]
    tasks += [build_wgslsmith]
elif args.task == "harness":
    if not use_prebuilt_dawn:
        tasks += [build_dawn]
    tasks += [build_harness]

for task in tasks:
    task()
