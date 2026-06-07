use std::collections::{BTreeMap, HashMap};
use std::io::{self, BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::{Parser, ValueEnum};
use crossbeam_channel::select;
use crossterm::event::KeyCode;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use eyre::eyre;
use harness_types::ConfigId;
use regex::Regex;
use serde::Serialize;
use tap::Tap;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};
use time::{format_description, OffsetDateTime, UtcOffset};
use tui::backend::{Backend, CrosstermBackend};
use tui::layout::Rect;
use tui::text::Spans;
use tui::widgets::{Block, Borders, Paragraph};
use tui::Terminal;

use crate::config::Config;
use crate::harness_runner::{
    self, get_targets, ConsensusEntry, ExecutionResult, Target, TargetPath,
};

#[derive(Clone, Serialize)]
struct FuzzerContext {
    name: String,
    kind: String,
    flags: Vec<String>,
    configs: Vec<String>,
    use_daemon: bool,
}

impl FuzzerContext {
    fn new(kind: String, options: &Options, config: &Config) -> Self {
        let name = options.server.clone().unwrap_or_else(|| {
            config.fuzzer.name.clone().unwrap_or_else(|| {
                std::env::var("HOSTNAME")
                    .or_else(|_| std::env::var("COMPUTERNAME"))
                    .unwrap_or_else(|_| "unknown-machine".to_string())
            })
        });

        let configs = options.configs.iter().map(|c| c.to_string()).collect();

        Self {
            name,
            kind,
            flags: std::env::args().collect(),
            configs,
            use_daemon: options.use_daemon,
        }
    }
}

#[derive(Copy, Clone, ValueEnum)]
enum SaveStrategy {
    All,
    Crashes,
    Mismatches,
    /// Don't save any test cases - useful for debugging.
    Debug,
}

#[derive(Parser)]
pub struct Options {
    /// Path to directory in which to save failing test cases.
    #[clap(short, long, action, default_value = "out")]
    output: PathBuf,

    /// Strategy to use when determining which test cases to save.
    ///
    /// Note that `all` will still ignore crashes based on the `--ignore` option, if it is provided.
    #[clap(long, action, action, default_value = "all")]
    strategy: SaveStrategy,

    /// Regex for ignoring certain types of crashes.
    ///
    /// This will be matched against the stderr output from the test harness.
    #[clap(long, action)]
    ignore: Vec<Regex>,

    /// Address of harness server.
    #[clap(short, long, action)]
    pub server: Option<String>,

    #[clap(long, action)]
    enable_pointers: bool,

    #[clap(long = "gen-ext", action)]
    pub extensions: Vec<generator::GeneratorExtension>,

    #[clap(short, long = "config", action)]
    pub configs: Vec<ConfigId>,

    #[clap(short = 't', long = "target", action)]
    pub targets: Vec<TargetPath>,

    /// Disable the fancy terminal dashboard UI.
    #[clap(long, action)]
    disable_tui: bool,

    /// Whether to save random failures (other than execution failures or buffer mismatches).
    ///
    /// This is mostly for debugging.
    #[clap(long, action)]
    save_failures: bool,

    /// (Local execution only) Limit the number of parallel configurations executing at once.
    ///
    /// If not provided, execution will spawn a thread for every configuration.
    #[clap(long, short = 'j', action)]
    local_parallelism: Option<usize>,

    /// (Local execution only) Timeout in seconds.
    ///
    /// Use 0 to disable the timeout. Note that the timeout is per-execution rather than a global timeout.
    #[clap(long, action)]
    local_timeout: Option<u64>,

    #[clap(long, action, default_value = "false", name = "use_daemon_flag")]
    pub use_daemon: bool,

    #[clap(long, action, requires = "use_daemon_flag")]
    pub daemon_port: Option<u16>,

    #[clap(long, action, default_value = "false")]
    unstable_float: bool,

    #[clap(long, action)]
    pub perf_file: Option<PathBuf>,

    #[clap(long, action)]
    pub compile_only: bool,
}

impl Options {
    pub fn to_cmd(&self) -> Vec<String> {
        let mut exec_params = Vec::new();

        if let Some(timeout) = self.local_timeout {
            exec_params.push("--timeout".to_string());
            exec_params.push(timeout.to_string());
        }

        if let Some(parallelism) = self.local_parallelism {
            exec_params.push("-j".to_string());
            exec_params.push(parallelism.to_string());
        }

        if self.use_daemon {
            exec_params.push("--use-daemon".to_string());
            if let Some(port) = self.daemon_port {
                exec_params.push("--daemon-port".to_string());
                exec_params.push(port.to_string());
            }
        }

        if let Some(perf_file) = &self.perf_file {
            exec_params.push("--perf-file".to_string());
            exec_params.push(perf_file.to_string_lossy().into_owned());
        }

        if self.compile_only {
            exec_params.push("--compile-only".to_string());
        }

        exec_params
    }
}

fn gen_shader(options: &Options) -> eyre::Result<String> {
    use rand::Rng;
    let stage = match rand::thread_rng().gen_range(0..4) {
        0 | 1 => "compute",
        2 => "vertex",
        _ => "fragment",
    };

    let output = Command::new(std::env::current_exe().unwrap())
        .arg("gen")
        .args(["--stage", stage])
        .args(["--block-min-stmts", "1"])
        .args(["--block-max-stmts", "2"])
        .args(["--max-fns", "3"])
        .tap_mut(|cmd| {
            if options.enable_pointers {
                cmd.arg("--enable-pointers");
            }
            for ext in &options.extensions {
                cmd.arg("--gen-ext");
                _ = match ext {
                    generator::GeneratorExtension::F16 => cmd.arg("f16"),
                    generator::GeneratorExtension::Subgroups => cmd.arg("subgroups"),
                }
            }
            if options.unstable_float {
                cmd.arg("--unstable-float");
            }
        })
        .stdout(Stdio::piped())
        .output()?;

    if !output.status.success() {
        return Err(eyre!("wgslsmith command failed"));
    }

    Ok(String::from_utf8(output.stdout)?)
}

fn recondition_shader(shader: &str, unstable_float: bool) -> eyre::Result<String> {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.arg("recondition");
    if unstable_float {
        cmd.arg("--unstable-float");
    }
    let mut reconditioner = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;

    {
        let stdin = reconditioner.stdin.take().unwrap();
        let mut writer = BufWriter::new(stdin);
        write!(writer, "{shader}")?;
        writer.flush()?;
    }

    let output = reconditioner.wait_with_output()?;
    if !output.status.success() {
        return Err(eyre!("reconditioner command failed"));
    }

    Ok(String::from_utf8(output.stdout)?)
}

impl ExecutionResult {
    fn should_save<'a>(
        &self,
        strategy: &SaveStrategy,
        mut ignore: impl Iterator<Item = &'a Regex>,
    ) -> bool {
        match self {
            ExecutionResult::Success(_) => false,
            // ExecutionResult::Timeout => false,
            ExecutionResult::Crash(output) => {
                matches!(strategy, SaveStrategy::All | SaveStrategy::Crashes)
                    && !ignore.any(|it| it.is_match(output))
            }
            ExecutionResult::Mismatch(_) => {
                matches!(strategy, SaveStrategy::All | SaveStrategy::Mismatches)
            }
        }
    }
}

static mut UTC_OFFSET: Option<UtcOffset> = None;

fn save_shader(
    out: &Path,
    shader: &str,
    reconditioned: &str,
    metadata: &str,
    output: Option<&str>,
    kind: Option<ExecutionResult>,
    info: &serde_json::Value,
) -> eyre::Result<()> {
    let now = OffsetDateTime::now_utc().to_offset(unsafe { UTC_OFFSET }.unwrap());
    let mut filename = now.format(&format_description::parse(
        "[year]-[month]/[day]/[hour]-[minute]-[second]",
    )?)?;

    if let Some(kind) = &kind {
        filename = format!("{filename}-{kind}");
    }

    let out = out.join(&filename);

    std::fs::create_dir_all(&out)?;

    std::fs::write(out.join("shader.wgsl"), shader)?;
    std::fs::write(out.join("reconditioned.wgsl"), reconditioned)?;
    std::fs::write(out.join("inputs.json"), metadata)?;

    if let Some(output) = output {
        std::fs::write(out.join("stderr.txt"), output.replace('\0', ""))?;
    }
    if let Some(ExecutionResult::Mismatch(consensus_vec)) = kind {
        std::fs::write(
            out.join("consensus.json"),
            consensus_vec
                .iter()
                .map(|e| format!("{:?} {:?}", e.configs, e.output))
                .collect::<Vec<_>>()
                .join("\n"),
        )?;
    }

    let info_str = serde_json::to_string_pretty(info)?;
    std::fs::write(out.join("info.json"), info_str)?;

    Ok(())
}

pub fn run(config: Config, options: Options) -> eyre::Result<()> {
    unsafe { UTC_OFFSET = Some(UtcOffset::current_local_offset()?) };

    let disable_tui = options.disable_tui;
    let targets = get_targets(&config, &options.server, &options.configs, &options.targets)?;

    let (worker_tx, worker_rx) = crossbeam_channel::bounded(1);

    std::thread::spawn(move || {
        worker(config, options, &targets, &mut |result| {
            worker_tx.send(result).unwrap()
        })
        .unwrap()
    });

    if disable_tui {
        while let Ok(msg) = worker_rx.recv() {
            match msg {
                WorkerMessage::Log(line) => println!("{line}"),
                WorkerMessage::Result(result) => println!(
                    "saved: {} (time: {:.3}s)",
                    result.saved,
                    result.duration.as_secs_f64()
                ),
            }
        }
    } else {
        enable_raw_mode()?;
        let stdout = io::stdout();
        let mut is_tui = true;
        let mut needs_render = true;

        let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        if is_tui {
            execute!(terminal.backend_mut(), EnterAlternateScreen)?;
        }

        let mut state = UiState::default();

        let (input_tx, input_rx) = crossbeam_channel::bounded(1);

        thread::spawn(move || {
            while let Ok(event) = crossterm::event::read() {
                if input_tx.send(event).is_err() {
                    break;
                }
            }
        });

        loop {
            if is_tui && needs_render {
                render_ui(&mut terminal, &state)?;
                needs_render = false;
            }

            select! {
                recv(input_rx) -> msg => {
                    if let Ok(crossterm::event::Event::Key(key)) = msg {
                        if let KeyCode::Char('q') = key.code {
                            break;
                        } else if let KeyCode::Char('t') = key.code {
                            is_tui = !is_tui;
                            if is_tui {
                                execute!(terminal.backend_mut(), EnterAlternateScreen)?;
                                terminal.clear()?;
                                needs_render = true;
                            } else {
                                execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
                                print!("Switched to log view. Press 't' to switch back to TUI, 'q' to quit.\r\n");
                                io::stdout().flush()?;
                            }
                        }
                    }

                    if is_tui {
                        needs_render = true;
                    }
                }
                recv(worker_rx) -> msg => {
                    match msg {
                        Ok(WorkerMessage::Log(line)) => {
                            if !is_tui {
                                print!("{}\r\n", line.replace('\n', "\r\n"));
                                io::stdout().flush()?;
                            }
                        },
                        Ok(WorkerMessage::Result(result)) => {
                            state.total += 1;
                            state.sum_time += result.duration;
                            *state
                                .time_buckets_ms
                                .entry(result.duration.as_millis() as u64)
                                .or_insert(0) += 1;

                            match result.kind {
                                WorkerResultKind::Success => state.success += 1,
                                WorkerResultKind::Crash => {
                                    state.crashes += 1;
                                    if result.saved {
                                        state.saved_crashes += 1;
                                    }
                                }
                                WorkerResultKind::Mismatch => {
                                    state.mismatches += 1;
                                    if result.saved {
                                        state.saved_mismatches += 1;
                                    }
                                }
                                WorkerResultKind::ReconditionFailure | WorkerResultKind::ExecutionFailure => {
                                    state.failures += 1
                                }
                            }

                            if is_tui {
                                needs_render = true;
                            } else {
                                print!(
                                    "saved: {} (time: {:.3}s)\r\n",
                                    result.saved,
                                    result.duration.as_secs_f64()
                                );
                                io::stdout().flush()?;
                            }
                        },
                        Err(_) => break,
                    }
                }
            }
        }

        if is_tui {
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        }
        disable_raw_mode()?;
        terminal.show_cursor()?;
    }

    Ok(())
}

enum WorkerMessage {
    Log(String),
    Result(WorkerResult),
}

struct WorkerResult {
    kind: WorkerResultKind,
    saved: bool,
    duration: Duration,
}

enum WorkerResultKind {
    Success,
    Crash,
    Mismatch,
    // Timeout,
    ReconditionFailure,
    ExecutionFailure,
}

fn worker(
    config: Config,
    options: Options,
    targets: &[Target],
    on_message: &mut dyn FnMut(WorkerMessage),
) -> eyre::Result<()> {
    loop {
        let mut logger = |line| on_message(WorkerMessage::Log(line));
        let start_time = Instant::now();

        match worker_iteration(&config, &options, targets, &mut logger) {
            Ok(mut result) => {
                result.duration = start_time.elapsed();
                on_message(WorkerMessage::Result(result));
            }
            Err(e) => {
                on_message(WorkerMessage::Log(format!("Iteration failed: {:#}", e)));

                on_message(WorkerMessage::Result(WorkerResult {
                    kind: WorkerResultKind::ExecutionFailure,
                    saved: false,
                    duration: start_time.elapsed(),
                }));

                continue;
            }
        }
    }
}

fn worker_iteration(
    config: &Config,
    options: &Options,
    targets: &[Target],
    logger: &mut dyn FnMut(String),
) -> eyre::Result<WorkerResult> {
    let shader = gen_shader(options)?;
    let (metadata, shader) = shader
        .split_once('\n')
        .ok_or_else(|| eyre!("expected first line of shader to be a JSON metadata comment"))?;

    let metadata = metadata.trim_start_matches("//").trim();

    let reconditioned = match recondition_shader(shader, options.unstable_float) {
        Ok(reconditioned) => format!(
            "{}\n{reconditioned}",
            shader
                .split_once('\n')
                .ok_or_else(|| eyre!("expected first line of shader to be a seed comment"))?
                .0
        ),
        Err(_) => {
            eprintln!("reconditioner command failed, ignoring");
            return Ok(WorkerResult {
                kind: WorkerResultKind::ReconditionFailure,
                saved: false,
                duration: Duration::ZERO,
            });
        }
    };

    // Save in case the system crashes
    std::fs::write("last.wgsl", &reconditioned)?;

    let mut result = ExecutionResult::Success(None);
    let mut buffers_to_configs: HashMap<Vec<u8>, Vec<String>> = HashMap::new();

    for target in targets {
        let exec_result = harness_runner::exec_shader(
            target,
            &reconditioned,
            metadata,
            &options.to_cmd(),
            &mut *logger,
        );

        result = match exec_result {
            Ok(result) => result,
            Err(e) => {
                if options.save_failures {
                    save_shader(
                        &options.output,
                        shader,
                        &reconditioned,
                        metadata,
                        Some(&format!("{e:#?}")),
                        None,
                        &serde_json::json!(FuzzerContext::new(
                            "failure".to_string(),
                            options,
                            config
                        )),
                    )?;
                }
                return Ok(WorkerResult {
                    kind: WorkerResultKind::ExecutionFailure,
                    saved: false,
                    duration: Duration::ZERO,
                });
            }
        };

        if let ExecutionResult::Success(ref e) = result {
            // if not timeout/empty result
            if let Some(entry) = e.as_ref() {
                buffers_to_configs
                    .entry(entry.output.clone())
                    .or_default()
                    .extend(entry.configs.clone());
            }
        } else {
            break;
        }
    }

    // harness mismatch
    if let ExecutionResult::Success(_) = result {
        if buffers_to_configs.len() > 1 {
            let mut stdout = StandardStream::stdout(ColorChoice::Auto);
            let consensus_vec: Vec<ConsensusEntry> = buffers_to_configs
                .iter()
                .map(|(buf, configs)| ConsensusEntry {
                    output: buf.clone(),
                    configs: configs.clone(),
                })
                .collect();
            result = ExecutionResult::Mismatch(consensus_vec);

            let mut red = ColorSpec::new();
            red.set_fg(Some(Color::Red));
            stdout.set_color(&red)?;

            writeln!(stdout, "harness mismatch\n")?;
            stdout.reset()?;
        }
    }

    let result_kind = match result {
        ExecutionResult::Success(_) => WorkerResultKind::Success,
        ExecutionResult::Crash(_) => WorkerResultKind::Crash,
        ExecutionResult::Mismatch(_) => WorkerResultKind::Mismatch,
        // ExecutionResult::Timeout => WorkerResultKind::Timeout,
    };

    let mut output = None;
    if let ExecutionResult::Crash(out) = &result {
        output = Some(out.as_str());
    }

    let should_save = result.should_save(
        &options.strategy,
        options.ignore.iter().chain(&config.fuzzer.ignore),
    );

    if should_save {
        save_shader(
            &options.output,
            shader,
            &reconditioned,
            metadata,
            output,
            Some(result.clone()),
            &serde_json::json!(FuzzerContext::new(
                match result_kind {
                    WorkerResultKind::Success => "success",
                    WorkerResultKind::Crash => "crash",
                    WorkerResultKind::Mismatch => "mismatch",
                    WorkerResultKind::ReconditionFailure => "recondition_failure",
                    WorkerResultKind::ExecutionFailure => "execution_failure",
                }
                .to_string(),
                options,
                config
            )),
        )?;
    }

    Ok(WorkerResult {
        kind: result_kind,
        saved: should_save,
        duration: Duration::ZERO,
    })
}

struct UiState {
    total: usize,
    success: usize,
    timeouts: usize,
    crashes: usize,
    saved_crashes: usize,
    mismatches: usize,
    saved_mismatches: usize,
    failures: usize,
    sum_time: Duration,
    time_buckets_ms: BTreeMap<u64, usize>,
    start_time: Instant,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            total: 0,
            success: 0,
            timeouts: 0,
            crashes: 0,
            saved_crashes: 0,
            mismatches: 0,
            saved_mismatches: 0,
            failures: 0,
            sum_time: Duration::ZERO,
            time_buckets_ms: BTreeMap::new(),
            start_time: Instant::now(),
        }
    }
}

fn render_ui<B: Backend>(terminal: &mut Terminal<B>, state: &UiState) -> eyre::Result<()> {
    fn pc(a: usize, b: usize) -> f32 {
        if b == 0 {
            0.0
        } else {
            a as f32 / b as f32 * 100.0
        }
    }

    terminal.draw(|f| {
        let count = state.total;
        let success = state.success;
        let crashes = state.crashes;
        let saved_crashes = state.saved_crashes;
        let mismatches = state.mismatches;
        let saved_mismatches = state.saved_mismatches;
        let timeouts = state.timeouts;
        let failures = state.failures;

        let avg = if count == 0 {
            0.0
        } else {
            state.sum_time.as_secs_f64() / count as f64
        };

        let p95_idx = (count as f64 * 0.95).ceil() as usize;
        let mut current_idx = 0;
        let mut p95_ms = 0;
        for (&ms, &c) in &state.time_buckets_ms {
            current_idx += c;
            if current_idx >= p95_idx {
                p95_ms = ms;
                break;
            }
        }
        let p95_sec = p95_ms as f64 / 1000.0;

        let wall_secs = state.start_time.elapsed().as_secs();
        let hours = wall_secs / 3600;
        let mins = (wall_secs % 3600) / 60;
        let secs = wall_secs % 60;

        #[rustfmt::skip]
        let lines = vec![
            Spans::from(format!("total:      {count}")),
            Spans::from(format!("ok:         {success} ({:.2}%)", pc(success, count))),
            Spans::from(format!("crashes:    {crashes} ({:.2}%)", pc(crashes, count))),
            Spans::from(format!("  saved:    {saved_crashes} ({:.2}%)", pc(saved_crashes, crashes))),
            Spans::from(format!("mismatches: {mismatches} ({:.2}%)", pc(mismatches, count))),
            Spans::from(format!("  saved:    {saved_mismatches} ({:.2}%)", pc(saved_mismatches, mismatches))),
            Spans::from(format!("timeouts:   {timeouts} ({:.2}%)", pc(timeouts, count))),
            Spans::from(format!("failures:   {failures} ({:.2}%)", pc(failures, count))),
            Spans::from(""),
            Spans::from(format!("total time: {:02}:{:02}:{:02}", hours, mins, secs)),
            Spans::from(format!("avg time:   {:.3}s", avg)),
            Spans::from(format!("p95 time:   {:.3}s", p95_sec)),
        ];

        let line_count = lines.len();
        let mut text_width = 0;
        for line in &lines {
            text_width = text_width.max(line.width());
        }

        let block = Block::default()
            .title(" wgslsmith - fuzzer ")
            .borders(Borders::ALL);

        let text = Paragraph::new(lines);

        let frame_area = f.size();
        let text_area = Rect::new(
            frame_area.x + ((frame_area.width - frame_area.x) / 2 - (text_width as u16 / 2)),
            frame_area.y + ((frame_area.height - frame_area.y) / 2 - (line_count as u16 / 2)),
            text_width as u16,
            line_count as u16,
        );

        f.render_widget(block, frame_area);
        f.render_widget(text, text_area);
    })?;
    Ok(())
}
