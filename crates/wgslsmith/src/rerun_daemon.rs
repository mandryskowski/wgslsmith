use std::path::{Path, PathBuf};

use clap::Parser;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::harness_runner::{self, ExecutionResult};

#[derive(Parser)]
pub struct Options {
    #[clap(action)]
    pub dir: PathBuf,

    /// Only re-run if the existing stderr.txt matches this regex.
    #[clap(long, action)]
    pub regex: Option<Regex>,
}

#[derive(Deserialize, Serialize)]
struct Info {
    configs: Vec<String>,
    flags: Vec<String>,
    kind: String,
    name: String,
    use_daemon: bool,
}

pub fn run(config: &Config, options: Options) -> eyre::Result<()> {
    let current_fuzzer_name = config.fuzzer.name.clone().unwrap_or_else(|| {
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown-machine".to_string())
    });

    let mut dirs_to_visit = vec![options.dir.clone()];
    while let Some(dir) = dirs_to_visit.pop() {
        if dir.is_dir() {
            if dir.join("info.json").exists() {
                process_dir(config, &options, &dir, &current_fuzzer_name)?;
            } else {
                for entry in std::fs::read_dir(&dir)? {
                    let entry = entry?;
                    if entry.path().is_dir() {
                        dirs_to_visit.push(entry.path());
                    }
                }
            }
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
    let mut info: Info = serde_json::from_str(&info_file)?;

    if info.name != current_fuzzer_name && !config.remotes.contains_key(&info.name) {
        println!(
            "Warning: Fuzzer name mismatch in {}: expected {}, found {} (and no matching remote found in config)",
            dir.display(),
            current_fuzzer_name,
            info.name
        );
    }

    if info.kind == "crash" && info.use_daemon {
        if let Some(ref re) = options.regex {
            let stderr_path = dir.join("stderr.txt");
            if !stderr_path.exists() {
                return Ok(());
            }
            let stderr_content = std::fs::read_to_string(&stderr_path)?;
            if !re.is_match(&stderr_content) {
                return Ok(());
            }
        }

        let top_options = crate::Options::try_parse_from(&info.flags)?;
        let mut fuzzer_options = match top_options.cmd {
            crate::Cmd::Fuzz(options) => options,
            _ => eyre::bail!("Expected fuzz command in flags"),
        };

        fuzzer_options.use_daemon = false;
        let cmd_args = fuzzer_options.to_cmd();

        let configs: eyre::Result<Vec<harness_types::ConfigId>> = info
            .configs
            .iter()
            .map(|c| c.parse().map_err(|e| eyre::eyre!("{e}")))
            .collect();
        let configs = configs?;

        let mut server = fuzzer_options.server.clone();

        if info.name != current_fuzzer_name {
            server = Some(info.name.clone());
            fuzzer_options.targets.clear();
        }

        let targets =
            harness_runner::get_targets(config, &server, &configs, &fuzzer_options.targets)?;

        let reconditioned_path = dir.join("reconditioned.wgsl");
        let inputs_path = dir.join("inputs.json");

        if !reconditioned_path.exists() || !inputs_path.exists() {
            return Ok(());
        }

        let reconditioned = std::fs::read_to_string(&reconditioned_path)?;
        let metadata = std::fs::read_to_string(&inputs_path)?;

        let mut all_crashes_output = String::new();
        let mut still_crash = false;

        println!("running {:?}", targets);

        for target in targets {
            let exec_result =
                harness_runner::exec_shader(&target, &reconditioned, &metadata, &cmd_args, |_| {});

            match exec_result {
                Ok(ExecutionResult::Crash(output)) => {
                    still_crash = true;
                    all_crashes_output.push_str(&output);
                }
                Err(e) => {
                    still_crash = true;
                    all_crashes_output.push_str(&format!("{e:#?}\n"));
                }
                _ => {}
            }
        }

        if still_crash {
            info.use_daemon = false;
            info.flags.retain(|flag| flag != "--use-daemon");

            std::fs::write(dir.join("stderr.txt"), all_crashes_output.replace('\0', ""))?;
            std::fs::write(&info_path, serde_json::to_string_pretty(&info)?)?;
            println!("Rewrote stderr.txt for {}", dir.display());
        } else {
            println!("No longer a crash for {}", dir.display());
            std::fs::remove_dir_all(dir)?;
        }
    }

    Ok(())
}
