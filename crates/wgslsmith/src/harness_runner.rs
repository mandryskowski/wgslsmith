use crate::config::Config;
use bincode::{Decode, Encode};
use eyre::eyre;
use harness_types::ConfigId;
use std::fmt::{Display, Write as _};
use std::io::{self, BufRead, BufReader, BufWriter, Write as _};
use std::path::PathBuf;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::str::FromStr;
use std::thread;
use tap::Tap;

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ExecutionResult {
    Success(Vec<u8>),
    Crash(String),
    Mismatch,
    // TODO: Detect timeouts from running harness
    // Might not actually be necessary since it's probably fine to treat them as successful runs
    // Timeout,
}

impl Display for ExecutionResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionResult::Success(_) => write!(f, "success"),
            ExecutionResult::Crash(_) => write!(f, "crash"),
            ExecutionResult::Mismatch => write!(f, "mismatch"),
            // ExecutionResult::Timeout => write!(f, "timeout"),
        }
    }
}

#[derive(Clone)]
pub enum Harness {
    Local(PathBuf),
    Remote(String),
}

#[derive(Clone, Debug, Decode, Encode)]
pub struct TargetPath {
    harness_name: String,
    configs: Vec<ConfigId>,
}

impl FromStr for TargetPath {
    type Err = eyre::Error;

    fn from_str(arg: &str) -> Result<TargetPath, Self::Err> {
        let (config_str, harness_name) = arg
            .split_once('@')
            .ok_or_else(|| eyre!("Target format must be configs@address"))?;

        let configs: Vec<ConfigId> = if config_str.is_empty() {
            vec![]
        } else {
            config_str
                .split(',')
                .map(|s| s.trim().parse::<ConfigId>())
                .collect::<Result<_, _>>()
                .map_err(|s| eyre!(s))?
        };

        Ok(TargetPath {
            harness_name: harness_name.to_owned(),
            configs,
        })
    }
}

#[derive(Clone)]
pub struct Target {
    pub harness: Harness,
    pub configs: Vec<ConfigId>,
}

impl Target {
    pub fn from_path(target_path: TargetPath, config: &Config) -> eyre::Result<Self> {
        let harness = match target_path.harness_name.as_str() {
            "local" => Harness::Local(
                config
                    .harness
                    .path
                    .clone()
                    .map(Ok)
                    .unwrap_or_else(std::env::current_exe)?,
            ),
            server => Harness::Remote(server.to_owned()),
        };

        Ok(Target {
            harness,
            configs: target_path.configs,
        })
    }

    pub fn new(harness: Harness, configs: Vec<ConfigId>) -> Self {
        Self { harness, configs }
    }
}

pub fn exec_shader(
    harness: &Harness,
    configs: Vec<ConfigId>,
    shader: &str,
    metadata: &str,
    mut logger: impl FnMut(String),
) -> eyre::Result<ExecutionResult> {
    exec_shader_impl(harness, configs, shader, metadata, &mut logger)
}

fn exec_shader_impl(
    harness: &Harness,
    configs: Vec<ConfigId>,
    shader: &str,
    metadata: &str,
    logger: &mut dyn FnMut(String),
) -> eyre::Result<ExecutionResult> {
    let mut cmd = match harness {
        Harness::Local(harness_path) => Command::new(harness_path).tap_mut(|cmd| {
            cmd.args(["run", "-", metadata]);
        }),
        Harness::Remote(remote) => Command::new(std::env::current_exe()?).tap_mut(|cmd| {
            cmd.args(["remote", remote, "run", "-", metadata]);
        }),
    };

    for config in configs {
        cmd.args(["-c", &config.to_string()]);
    }

    cmd.args(["--print-output-if-ok"]);

    let mut harness = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()?;

    {
        let stdin = harness.stdin.take().unwrap();
        let mut writer = BufWriter::new(stdin);
        write!(writer, "{shader}")?;
        writer.flush()?;
    }

    let mut output = String::new();
    let mut consensus_str = None;

    let status = wait_for_child_with_line_logger(harness, &mut |_, line| {
        if line.starts_with("output-consensus: ") {
            consensus_str = Some(line.trim_start_matches("output-consensus: ").to_string());
            return;
        }
        writeln!(output, "{line}").unwrap();
        logger(line);
    })?;

    let consensus: Vec<_> = consensus_str
        .as_ref()
        .map(|s| {
            s.trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|x| x.trim().parse::<u8>().expect("Invalid byte string"))
                .collect()
        })
        .unwrap_or_default();

    let result = match status.code() {
        None => return Err(eyre!("failed to get harness exit code")),
        Some(0) => ExecutionResult::Success(consensus),
        Some(1) => ExecutionResult::Mismatch,
        Some(101) => ExecutionResult::Crash(output),
        Some(code) => return Err(eyre!("harness exited with unrecognised code `{code}`")),
    };

    Ok(result)
}

#[derive(PartialEq, Eq)]
enum StdioKind {
    Stdout,
    Stderr,
}

fn wait_for_child_with_line_logger(
    mut child: Child,
    logger: &mut dyn FnMut(StdioKind, String),
) -> Result<ExitStatus, io::Error> {
    let (tx, rx) = crossbeam_channel::unbounded();

    child.stdout.take().map(|stdout| {
        thread::spawn({
            let tx = tx.clone();
            move || {
                BufReader::new(stdout)
                    .lines()
                    .map_while(Result::ok)
                    .try_for_each(|line| tx.send((StdioKind::Stdout, line)))
                    .unwrap();
            }
        })
    });

    child.stderr.take().map(|stderr| {
        thread::spawn({
            let tx = tx.clone();
            move || {
                BufReader::new(stderr)
                    .lines()
                    .map_while(Result::ok)
                    .try_for_each(|line| tx.send((StdioKind::Stderr, line)))
                    .unwrap();
            }
        })
    });

    drop(tx);

    while let Ok((kind, line)) = rx.recv() {
        logger(kind, line);
    }

    child.wait()
}
