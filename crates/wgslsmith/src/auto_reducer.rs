use crate::harness_runner::{self, ExecutionResult};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::{Parser, ValueEnum};
use serde::Deserialize;

use crate::config::Config;

#[derive(Parser)]
pub struct Options {
    /// Directory to search for fuzzer outputs.
    #[clap(action)]
    pub dir: PathBuf,

    /// Filter by kind.
    #[clap(long, value_enum, action)]
    pub filter: Option<FilterKind>,

    /// Address of harness server to use instead of local.
    #[clap(short, long, action)]
    pub server: Option<String>,

    /// Number of attempts to check if the bug reproduces (useful for flaky bugs).
    #[clap(long, action)]
    pub attempts: Option<u32>,
}

#[derive(ValueEnum, Clone, PartialEq, Eq, Debug)]
pub enum FilterKind {
    Crash,
    Mismatch,
}

#[derive(Deserialize)]
struct Info {
    configs: Vec<String>,
    kind: String,
    name: String,
}

pub fn run(config: &Config, options: Options) -> eyre::Result<()> {
    let current_fuzzer_name = config.fuzzer.name.clone().unwrap_or_else(|| {
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown-machine".to_string())
    });

    let mut dirs_to_visit = vec![options.dir.clone()];

    while let Some(dir) = dirs_to_visit.pop() {
        if !dir.is_dir() {
            continue;
        }

        let info_path = dir.join("info.json");
        let original_shader_path = dir.join("shader.wgsl");
        let reconditioned_shader_path = dir.join("reconditioned.wgsl");

        if info_path.exists() && original_shader_path.exists() && reconditioned_shader_path.exists()
        {
            process_dir(config, &options, &dir, &current_fuzzer_name)?;
        } else {
            let mut subdirs = vec![];
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    subdirs.push(entry.path());
                }
            }
            subdirs.sort();
            subdirs.reverse();
            dirs_to_visit.extend(subdirs);
        }
    }
    Ok(())
}

fn process_dir(
    config: &Config,
    options: &Options,
    dir: &Path,
    current_fuzzer_name: &str,
) -> eyre::Result<()> {
    let info_path = dir.join("info.json");
    let info_file = std::fs::read_to_string(&info_path)?;
    let info: Info = match serde_json::from_str(&info_file) {
        Ok(info) => info,
        Err(_) => return Ok(()),
    };

    let kind = match info.kind.as_str() {
        "crash" => FilterKind::Crash,
        "mismatch" => FilterKind::Mismatch,
        _ => return Ok(()),
    };

    if let Some(filter) = &options.filter {
        if kind != *filter {
            return Ok(());
        }
    }

    let reduced_dir = dir.join("reduced");
    if reduced_dir.exists() {
        println!("Skipping {} (already reduced)", dir.display());
        return Ok(());
    }

    if info.name != current_fuzzer_name && !config.remotes.contains_key(&info.name) {
        println!(
            "Warning: Fuzzer name mismatch in {}: expected {}, found {} (and no matching remote found in config)",
            dir.display(),
            current_fuzzer_name,
            info.name
        );
    }

    let mut harness_server = options.server.clone();
    if info.name != current_fuzzer_name {
        harness_server = Some(info.name.clone());
    }
    let harness_server = harness_server.as_deref().unwrap_or("local");

    let original_shader_path = dir.join("shader.wgsl");
    let reconditioned_shader_path = dir.join("reconditioned.wgsl");
    let exe = std::env::current_exe()?;

    println!("========================================");
    println!("Processing {} ({:?})", dir.display(), kind);

    if kind == FilterKind::Crash {
        let stderr_path = dir.join("stderr.txt");
        if !stderr_path.exists() {
            println!("No stderr.txt found, skipping.");
            return Ok(());
        }
        let stderr = std::fs::read_to_string(&stderr_path)?;
        println!("Crash output (from fuzzer):\n{}", stderr);

        println!("\nReproducing crash live...");
        let inputs_path = dir.join("inputs.json");
        if inputs_path.exists() && reconditioned_shader_path.exists() {
            let metadata = std::fs::read_to_string(&inputs_path)?;
            let reconditioned = std::fs::read_to_string(&reconditioned_shader_path)?;

            let parsed_configs: eyre::Result<Vec<_>> = info
                .configs
                .iter()
                .map(|c| c.parse().map_err(|e| eyre::eyre!("{e}")))
                .collect();

            if let Ok(configs) = parsed_configs {
                let mut server = options.server.clone();
                if info.name != current_fuzzer_name {
                    server = Some(info.name.clone());
                }

                if let Ok(targets) = harness_runner::get_targets(config, &server, &configs, &[]) {
                    for target in targets {
                        let configs_str = target
                            .configs
                            .iter()
                            .map(|c| c.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        println!("--- Running config: {} ---", configs_str);
                        let result = harness_runner::exec_shader(
                            &target,
                            &reconditioned,
                            &metadata,
                            &[],
                            |line| {
                                println!("{}", line);
                            },
                        );

                        match result {
                            Ok(ExecutionResult::Crash(_)) => {}
                            Ok(res) => {
                                println!("Live result: {}", res);
                            }
                            Err(e) => {
                                println!("Execution error: {}", e);
                            }
                        }
                    }
                } else {
                    println!("Failed to resolve targets.");
                }
            }
        }

        println!();
        print!("Reduce this crash? [y/N]: ");
        io::stdout().flush()?;
        let mut reduce_ans = String::new();
        io::stdin().read_line(&mut reduce_ans)?;
        if !reduce_ans.trim().eq_ignore_ascii_case("y") {
            println!("Skipping.");
            return Ok(());
        }

        print!("Enter regex to match this crash (or leave empty to skip): ");
        io::stdout().flush()?;
        let mut regex_str = String::new();
        io::stdin().read_line(&mut regex_str)?;
        let regex_str = regex_str.trim_end_matches(&['\r', '\n'][..]).to_string();

        print!("Enter inverse regex (or leave empty to skip): ");
        io::stdout().flush()?;
        let mut inverse_regex_str = String::new();
        io::stdin().read_line(&mut inverse_regex_str)?;
        let inverse_regex_str = inverse_regex_str
            .trim_end_matches(&['\r', '\n'][..])
            .to_string();

        let mut target_config = None;
        for c in &info.configs {
            let mut cmd = Command::new(&exe);
            cmd.arg("test")
                .arg("crash")
                .arg(&original_shader_path)
                .arg("--config")
                .arg(c);

            if harness_server != "local" {
                cmd.arg("--server").arg(harness_server);
            }

            cmd.arg("--regex").arg(&regex_str);
            if !inverse_regex_str.is_empty() {
                cmd.arg("--inverse-regex").arg(&inverse_regex_str);
            }

            cmd.arg("--quiet");

            if let Some(attempts) = options.attempts {
                cmd.arg("--attempts").arg(attempts.to_string());
            }

            let status = cmd.status()?;
            if status.success() {
                target_config = Some(c.clone());
                break;
            }
        }

        if let Some(c) = target_config {
            println!("Found crashing config: {}", c);
            println!("Starting reduction...");
            let mut cmd = Command::new(&exe);
            cmd.arg("reduce")
                .arg("crash")
                .arg(&original_shader_path)
                .arg("--config")
                .arg(&c);

            if harness_server != "local" {
                cmd.arg("--server").arg(harness_server);
            }

            cmd.arg("--regex").arg(&regex_str);
            if !inverse_regex_str.is_empty() {
                cmd.arg("--inverse-regex").arg(&inverse_regex_str);
            }

            if let Some(attempts) = options.attempts {
                cmd.arg("--attempts").arg(attempts.to_string());
            }

            cmd.status()?;
        } else {
            println!("Could not reproduce crash with any config.");
        }
    } else if kind == FilterKind::Mismatch {
        let mut target_configs = None;
        'outer: for i in 0..info.configs.len() {
            for j in (i + 1)..info.configs.len() {
                let c1 = &info.configs[i];
                let c2 = &info.configs[j];

                let mut cmd = Command::new(&exe);
                cmd.arg("test")
                    .arg("mismatch")
                    .arg(&original_shader_path)
                    .arg("-t")
                    .arg(format!("{}@{}", c1, harness_server))
                    .arg("-t")
                    .arg(format!("{}@{}", c2, harness_server))
                    .arg("--quiet");

                if let Some(attempts) = options.attempts {
                    cmd.arg("--attempts").arg(attempts.to_string());
                }

                let status = cmd.status()?;
                if status.success() {
                    target_configs = Some((c1.clone(), c2.clone()));
                    break 'outer;
                }
            }
        }

        if let Some((c1, c2)) = target_configs {
            println!("Found mismatching configs: {} and {}", c1, c2);
            println!("Starting reduction...");
            let mut cmd = Command::new(&exe);
            cmd.arg("reduce")
                .arg("mismatch")
                .arg(&original_shader_path)
                .arg("-t")
                .arg(format!("{}@{}", c1, harness_server))
                .arg("-t")
                .arg(format!("{}@{}", c2, harness_server));

            if let Some(attempts) = options.attempts {
                cmd.arg("--attempts").arg(attempts.to_string());
            }

            cmd.status()?;
        } else {
            println!("Could not reproduce mismatch with any pair of configs.");
        }
    }

    Ok(())
}
