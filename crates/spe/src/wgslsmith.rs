use ast::EnableExtension;
use harness_types::ConfigId;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::process;

pub fn recondition_shader_src(wgslsmith_exe: &Path, src: &str) -> Option<String> {
    let mut recond_cmd = process::Command::new(wgslsmith_exe);
    recond_cmd.arg("recondition").arg("-");

    recond_cmd
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped());

    let recond_output = recond_cmd.spawn().and_then(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(src.as_bytes())?;
        }
        child.wait_with_output()
    });

    match recond_output {
        Ok(out) if out.status.success() => Some(String::from_utf8_lossy(&out.stdout).into_owned()),
        _ => None,
    }
}

pub fn test_shader_has_outputs(
    wgslsmith_exe: &Path,
    server: Option<&String>,
    configs: &[ConfigId],
    parallelism: Option<usize>,
    use_daemon: bool,
    shader_src: &str,
    inputs_json: Option<&str>,
) -> bool {
    let mut cmd = process::Command::new(wgslsmith_exe);
    if let Some(s) = server {
        cmd.arg("remote").arg(s);
    }
    cmd.arg("run");
    for config in configs {
        cmd.arg("-c").arg(config.to_string());
    }

    cmd.arg("-j").arg(parallelism.unwrap_or(2).to_string());

    if use_daemon {
        cmd.arg("--use-daemon");
    }

    cmd.arg("-");
    if let Some(inputs) = inputs_json {
        cmd.arg(inputs);
    }

    cmd.stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped());

    if let Ok(mut child) = cmd.spawn() {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(shader_src.as_bytes());
        }

        if let Ok(out) = child.wait_with_output() {
            let stdout_str = String::from_utf8_lossy(&out.stdout);
            let stderr_str = String::from_utf8_lossy(&out.stderr);
            return stdout_str.contains("outputs (") || stderr_str.contains("outputs (");
        }
    }
    false
}

pub fn run_compile(
    wgslsmith_exe: &Path,
    shader_src: &str,
    backend: &str,
    compiler: &str,
    failures_log: &mut dyn Write,
    context: &str,
) -> bool {
    let mut cmd = process::Command::new(wgslsmith_exe);
    cmd.arg("compile")
        .arg("--backend")
        .arg(backend)
        .arg("--compiler")
        .arg(compiler)
        .arg("--validate-output")
        .arg("-");

    cmd.stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped());

    let output = cmd.spawn().and_then(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(shader_src.as_bytes())?;
        }
        child.wait_with_output()
    });

    match output {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                writeln!(
                    failures_log,
                    "[{}] Failed compile (--backend {} --compiler {}) {}\nStdout: {}\nStderr: {}",
                    crate::stats::current_timestamp(),
                    backend,
                    compiler,
                    context,
                    stdout,
                    stderr
                )
                .unwrap();
                false
            } else {
                true
            }
        }
        Err(e) => {
            writeln!(
                failures_log,
                "[{}] Failed to execute wslinux compile (--backend {} --compiler {}) {}\nError: {}",
                crate::stats::current_timestamp(),
                backend,
                compiler,
                context,
                e
            )
            .unwrap();
            false
        }
    }
}

pub fn do_healthcheck(wgslsmith_exe: &Path, server: &str, configs: &[ConfigId]) {
    let mut cmd = process::Command::new(wgslsmith_exe);
    cmd.arg("remote").arg(server).arg("run");

    for config in configs {
        cmd.arg("-c").arg(config.to_string());
    }

    cmd.arg("-")
        .stdin(process::Stdio::piped())
        .stdout(process::Stdio::piped())
        .stderr(process::Stdio::piped());

    let output = cmd.spawn().and_then(|mut child| {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(b"@compute\n@workgroup_size(1)\nfn main() {}");
        }
        child.wait_with_output()
    });

    match output {
        Ok(out) if !out.status.success() => {
            println!("\nHealthcheck failed. Terminating.");
            crate::stats::print_stats();
            std::process::exit(1);
        }
        Err(e) => {
            println!("\nHealthcheck command failed: {}. Terminating.", e);
            crate::stats::print_stats();
            std::process::exit(1);
        }
        _ => {}
    }
}

pub fn check_extension_support(
    wgslsmith_exe: &Path,
    server: Option<&String>,
    configs: &[ConfigId],
) -> HashMap<EnableExtension, HashSet<ConfigId>> {
    let mut support_map = HashMap::new();

    println!("\nChecking extension support for configs...");

    for ext in <EnableExtension as strum::IntoEnumIterator>::iter() {
        let mut supported_configs = HashSet::new();
        let ext_str = ext.to_string();

        let shader_src = format!(
            "enable {};\n@compute @workgroup_size(1) fn main() {{}}",
            ext_str
        );

        for config in configs {
            let mut cmd = process::Command::new(wgslsmith_exe);
            if let Some(s) = server {
                cmd.arg("remote").arg(s);
            }
            cmd.arg("run").arg("-c").arg(config.to_string()).arg("-");

            cmd.stdin(process::Stdio::piped())
                .stdout(process::Stdio::piped())
                .stderr(process::Stdio::piped());

            if let Ok(mut child) = cmd.spawn() {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(shader_src.as_bytes());
                }
                if let Ok(out) = child.wait_with_output() {
                    if out.status.success() {
                        supported_configs.insert(config.clone());
                    }
                }
            }
        }
        println!(
            "  - {}: supported by {} config(s)",
            ext_str,
            supported_configs.len()
        );
        support_map.insert(ext, supported_configs);
    }
    println!();

    support_map
}
