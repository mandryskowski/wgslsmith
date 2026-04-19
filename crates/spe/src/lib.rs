pub mod enumerator;
mod vertex_reachable;

use clap::{Parser, Subcommand};
use harness_types::ConfigId;
use rand::seq::SliceRandom;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use time::{format_description, OffsetDateTime, UtcOffset};
use walkdir::WalkDir;

static STAT_FAILED_PARSE: AtomicUsize = AtomicUsize::new(0);
static STAT_FAILED_RECONDITION: AtomicUsize = AtomicUsize::new(0);
static STAT_FAILED_RUN: AtomicUsize = AtomicUsize::new(0);
static STAT_RUN_SUCCESS: AtomicUsize = AtomicUsize::new(0);
static STAT_SKIPPED_ONLY_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
static STAT_SUBSAMPLED: AtomicUsize = AtomicUsize::new(0);
static LAST_INDEX: AtomicUsize = AtomicUsize::new(0);
static DIRTY_STATS: AtomicBool = AtomicBool::new(false);

static OUT_DIR: OnceLock<PathBuf> = OnceLock::new();
static UTC_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

fn current_timestamp() -> String {
    let offset = *UTC_OFFSET.get().unwrap_or(&UtcOffset::UTC);
    let now = OffsetDateTime::now_utc().to_offset(offset);
    let format = format_description::parse("[hour]-[minute]-[second]").unwrap();
    now.format(&format).unwrap_or_default()
}

fn write_stats_json() {
    if let Some(out_dir) = OUT_DIR.get() {
        if !DIRTY_STATS.load(Ordering::SeqCst) {
            return;
        }

        let failed_parse = STAT_FAILED_PARSE.load(Ordering::SeqCst);
        let failed_recondition = STAT_FAILED_RECONDITION.load(Ordering::SeqCst);
        let failed_run = STAT_FAILED_RUN.load(Ordering::SeqCst);
        let run_success = STAT_RUN_SUCCESS.load(Ordering::SeqCst);
        let skipped_only_original = STAT_SKIPPED_ONLY_ORIGINAL.load(Ordering::SeqCst);
        let subsampled = STAT_SUBSAMPLED.load(Ordering::SeqCst);
        let last_idx = LAST_INDEX.load(Ordering::SeqCst);

        let args: Vec<String> = std::env::args().collect();

        let args_json = args
            .iter()
            .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");

        let json = format!(
            "{{\n  \"args\": [{}],\n  \"failed_parse\": {},\n  \"failed_recondition\": {},\n  \"failed_run\": {},\n  \"run_success\": {},\n  \"skipped_only_original\": {},\n  \"subsampled\": {},\n  \"last_handled_index\": {}\n}}",
            args_json, failed_parse, failed_recondition, failed_run, run_success, skipped_only_original, subsampled, last_idx
        );
        let _ = fs::write(out_dir.join("stats.json"), json);
    }
}

fn print_stats() {
    let failed_parse = STAT_FAILED_PARSE.load(Ordering::SeqCst);
    let failed_recondition = STAT_FAILED_RECONDITION.load(Ordering::SeqCst);
    let failed_run = STAT_FAILED_RUN.load(Ordering::SeqCst);
    let run_success = STAT_RUN_SUCCESS.load(Ordering::SeqCst);
    let skipped_only_original = STAT_SKIPPED_ONLY_ORIGINAL.load(Ordering::SeqCst);
    let subsampled = STAT_SUBSAMPLED.load(Ordering::SeqCst);
    let last_idx = LAST_INDEX.load(Ordering::SeqCst);

    let args: Vec<String> = std::env::args().collect();

    println!("\n=== SPE Execution Statistics ===");
    println!(
        "- Command ran:                                {}",
        args.join(" ")
    );
    println!(
        "- Original shaders failed to parse:           {}",
        failed_parse
    );
    println!(
        "- Original shaders failed to recondition:     {}",
        failed_recondition
    );
    println!(
        "- Original shaders failed to run:             {}",
        failed_run
    );
    println!(
        "- Original shaders run successfully:          {}",
        run_success
    );
    println!(
        "- Original shaders skipped (only original):   {}",
        skipped_only_original
    );
    println!(
        "- Original shaders subsampled:                {}",
        subsampled
    );
    println!("- Last handled shader index:                  {}", last_idx);
    println!("================================\n");

    write_stats_json();
}

fn load_stats(out_dir: &Path) -> usize {
    let stats_path = out_dir.join("stats.json");
    if let Ok(content) = fs::read_to_string(stats_path) {
        let parse_val = |key: &str| -> usize {
            content
                .lines()
                .find(|l| l.contains(key))
                .and_then(|l| l.split(':').nth(1))
                .and_then(|s| s.trim().trim_end_matches(',').parse().ok())
                .unwrap_or(0)
        };
        STAT_FAILED_PARSE.store(parse_val("\"failed_parse\""), Ordering::SeqCst);
        STAT_FAILED_RECONDITION.store(parse_val("\"failed_recondition\""), Ordering::SeqCst);
        STAT_FAILED_RUN.store(parse_val("\"failed_run\""), Ordering::SeqCst);
        STAT_RUN_SUCCESS.store(parse_val("\"run_success\""), Ordering::SeqCst);
        STAT_SKIPPED_ONLY_ORIGINAL.store(parse_val("\"skipped_only_original\""), Ordering::SeqCst);
        STAT_SUBSAMPLED.store(parse_val("\"subsampled\""), Ordering::SeqCst);

        return parse_val("\"last_handled_index\"");
    }
    0
}

#[derive(Parser, Debug)]
pub struct Options {
    #[clap(long, action, default_value = "false")]
    pub skip_original: bool,

    #[clap(subcommand)]
    pub cmd: SpeCommand,
}

#[derive(Subcommand, Debug)]
pub enum SpeCommand {
    /// Enumerate and test permutations for a single shader
    Enumerate(EnumerateOptions),
    /// Process a directory of shaders
    ProcessDir(DirOptions),
    /// Fuse shaders from a directory and test permutations
    Fuse(DirOptions),
}

#[derive(Parser, Debug)]
pub struct EnumerateOptions {
    /// Path to the WGSL shader
    pub shader_path: PathBuf,

    /// Run/print a specific enumeration of a shader
    #[clap(short = 'i', long)]
    pub index: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct DirOptions {
    /// Directory to scan for WGSL shaders
    pub directory: PathBuf,

    #[clap(long)]
    pub start_index: Option<usize>,

    /// Append to an existing directory's logs and resume its stats
    #[clap(long)]
    pub append_dir: Option<PathBuf>,

    #[clap(long, action, default_value = "false")]
    pub log_to_file: bool,

    /// Control parallelism passed to run
    #[clap(short = 'j', long)]
    pub parallelism: Option<usize>,

    #[clap(long, action, default_value = "false")]
    pub use_daemon: bool,

    /// Configurations to run (e.g., wgpu:dx12:5592)
    #[clap(short, long = "config", action)]
    pub configs: Vec<ConfigId>,

    /// Address of harness server.
    #[clap(short, long, action)]
    pub server: Option<String>,

    #[clap(long, action, default_value = "false")]
    pub msl_validate: bool,

    #[clap(long, action, default_value = "false")]
    pub skip_ext_filter: bool,

    /// File containing passed shaders to skip preprocessing
    #[clap(long)]
    pub passed_shaders: Option<PathBuf>,
}

fn run_compile(
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
                    current_timestamp(),
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
                current_timestamp(),
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

pub fn run(options: Options) {
    UTC_OFFSET.get_or_init(|| UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));

    if let Err(e) = ctrlc::set_handler(move || {
        println!("\nProcess interrupted by user.");
        print_stats();
        std::process::exit(0);
    }) {
        eprintln!("Warning: Failed to set Ctrl-C handler: {}", e);
    }

    let skip_original = options.skip_original;

    match options.cmd {
        SpeCommand::Enumerate(opt) => {
            run_enumerate(opt, skip_original);
        }
        SpeCommand::ProcessDir(opt) => {
            run_process_dir(opt, skip_original);
        }
        SpeCommand::Fuse(opt) => {
            run_fuse(opt, skip_original);
        }
    }
}

fn recondition_shader_src(wgslsmith_exe: &Path, src: &str) -> Option<String> {
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
        Ok(out) => {
            if out.status.success() {
                Some(String::from_utf8_lossy(&out.stdout).into_owned())
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

fn test_shader_has_outputs(
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

    cmd.arg("-j");
    if let Some(j) = parallelism {
        cmd.arg(j.to_string());
    } else {
        cmd.arg("2");
    }

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

fn run_fuse(opt: DirOptions, skip_original: bool) {
    let wgslsmith_exe = std::env::current_exe().expect("Failed to get current executable path");

    if let Some(server) = &opt.server {
        do_healthcheck(&wgslsmith_exe, server, &opt.configs);
    }

    let support_map = if opt.skip_ext_filter {
        std::collections::HashMap::new()
    } else {
        check_extension_support(&wgslsmith_exe, opt.server.as_ref(), &opt.configs)
    };

    let mut effective_start_index = opt.start_index;
    let log_to_file = opt.log_to_file || opt.append_dir.is_some();

    let (out_dir_opt, append) = if let Some(dir) = &opt.append_dir {
        let last_idx = load_stats(dir);
        if effective_start_index.is_none() && last_idx > 0 {
            effective_start_index = Some(last_idx);
        }
        (Some(dir.clone()), true)
    } else if log_to_file {
        let offset = *UTC_OFFSET.get().unwrap_or(&UtcOffset::UTC);
        let now = OffsetDateTime::now_utc().to_offset(offset);
        let format =
            format_description::parse("spe-[year]-[month]-[day]-[hour]-[minute]-[second]").unwrap();
        let dir_name = now.format(&format).unwrap();
        (Some(PathBuf::from(dir_name)), false)
    } else {
        (None, false)
    };

    if let Some(out_dir) = &out_dir_opt {
        fs::create_dir_all(out_dir).unwrap();
        let _ = OUT_DIR.set(out_dir.clone());
    }

    let (mut skipped_log, mut failures_log) = get_logs(log_to_file, out_dir_opt.as_deref(), append);

    let entries: Vec<_> = WalkDir::new(&opt.directory)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|ext| ext == "wgsl")
                && !p.to_string_lossy().ends_with(".expected.wgsl")
        })
        .collect();

    let mut working_modules = Vec::new();

    if let Some(passed_shaders_file) = &opt.passed_shaders {
        println!(
            "Loading passed shaders from {}",
            passed_shaders_file.display()
        );
        let content =
            fs::read_to_string(passed_shaders_file).expect("Failed to read passed shaders file");
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let path = Path::new(line);
            if let Ok(src) = fs::read_to_string(path) {
                if let Ok(module) = std::panic::catch_unwind(|| parser::parse(&src)) {
                    working_modules.push(module);
                }
            }
        }
        println!("Loaded {} working shaders.", working_modules.len());
    } else {
        let total_files = entries.len();
        println!(
            "Found {} shaders. Pre-filtering for validity...",
            total_files
        );

        let mut passed_paths = Vec::new();

        for (i, entry) in entries.into_iter().enumerate() {
            if i > 0 && i % 50 == 0 {
                println!(
                    "Pre-filtered {}/{} shaders ({} working)...",
                    i,
                    total_files,
                    working_modules.len()
                );
            }

            let path = entry.path();
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let module = match std::panic::catch_unwind(|| parser::parse(&content)) {
                Ok(m) => m,
                Err(_) => continue,
            };

            let mut input_buffers = fs::read_to_string(path.with_extension("in.json")).ok();
            if input_buffers.is_none() {
                input_buffers = generate_inputs_if_needed(&module);
            }

            if let Some(reconditioned_src) = recondition_shader_src(&wgslsmith_exe, &content) {
                if test_shader_has_outputs(
                    &wgslsmith_exe,
                    opt.server.as_ref(),
                    &opt.configs,
                    opt.parallelism,
                    opt.use_daemon,
                    &reconditioned_src,
                    input_buffers.as_deref(),
                ) {
                    working_modules.push(module);
                    passed_paths.push(path.to_path_buf());
                }
            }
        }

        let passed_file_path = out_dir_opt
            .as_deref()
            .unwrap_or(Path::new("."))
            .join("passed_shaders.txt");
        if let Ok(mut f) = fs::File::create(&passed_file_path) {
            for p in passed_paths {
                writeln!(f, "{}", p.display()).unwrap();
            }
            println!("Saved passed shaders to {}", passed_file_path.display());
        }

        println!(
            "Filtered down to {} working shaders.",
            working_modules.len()
        );
    }

    if working_modules.is_empty() {
        println!("No working shaders found. Exiting.");
        return;
    }

    let mut rng = rand::thread_rng();
    let mut file_num = effective_start_index.unwrap_or(0);
    let mut shaders_processed = 0;

    loop {
        file_num += 1;
        LAST_INDEX.store(file_num, Ordering::SeqCst);
        DIRTY_STATS.store(true, Ordering::SeqCst);

        use rand::Rng;
        let count = rng.gen_range(5..=10).min(working_modules.len());

        let mut chosen_indices: Vec<usize> = (0..working_modules.len()).collect();
        chosen_indices.shuffle(&mut rng);
        chosen_indices.truncate(count);

        let mut base_module = working_modules[chosen_indices[0]].clone();
        for &idx in &chosen_indices[1..] {
            let next_module = working_modules[idx].clone();
            base_module = fuse::fuse(base_module, next_module);
        }

        let fused_inputs = generate_inputs_if_needed(&base_module);

        {
            let mut out_str = String::new();
            ast::writer::Writer::default()
                .write_module(&mut out_str, &base_module)
                .unwrap();
            println!("shader is {}", out_str);
        }
        process_shader_core(
            &base_module,
            fused_inputs,
            format!("fused_shader_{}", file_num),
            format!("fused_{}", file_num),
            file_num,
            None,
            &opt,
            skip_original,
            &wgslsmith_exe,
            out_dir_opt.as_deref(),
            &mut *skipped_log,
            &mut *failures_log,
            &support_map,
            5, // max_enumerations for fuse mode
        );

        shaders_processed += 1;
        if shaders_processed % 50 == 0 {
            write_stats_json();

            if let Some(server) = &opt.server {
                do_healthcheck(&wgslsmith_exe, server, &opt.configs);
            }
        }
    }
}

fn get_logs(
    log_to_file: bool,
    out_dir: Option<&Path>,
    append: bool,
) -> (Box<dyn Write>, Box<dyn Write>) {
    let skipped_log: Box<dyn Write> = if log_to_file {
        Box::new(
            fs::OpenOptions::new()
                .create(true)
                .append(append)
                .write(true)
                .truncate(!append)
                .open(out_dir.unwrap().join("skipped.log"))
                .unwrap(),
        )
    } else {
        Box::new(io::stdout())
    };

    let failures_log: Box<dyn Write> = if log_to_file {
        Box::new(
            fs::OpenOptions::new()
                .create(true)
                .append(append)
                .write(true)
                .truncate(!append)
                .open(out_dir.unwrap().join("failures.log"))
                .unwrap(),
        )
    } else {
        Box::new(io::stderr())
    };

    (skipped_log, failures_log)
}

fn run_enumerate(opt: EnumerateOptions, skip_original: bool) {
    let content = match fs::read_to_string(&opt.shader_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to read {}: {}", opt.shader_path.display(), e);
            return;
        }
    };

    let module = match std::panic::catch_unwind(|| parser::parse(&content)) {
        Ok(m) => m,
        Err(_) => {
            eprintln!("Parse panic on: {}", opt.shader_path.display());
            return;
        }
    };

    let (holes, enumerations, original_assignment_idx) =
        match std::panic::catch_unwind(|| enumerator::get_enumerations(&module, None)) {
            Ok(res) => res,
            Err(_) => {
                eprintln!("Enumerate panic on: {}", opt.shader_path.display());
                return;
            }
        };

    if enumerations.is_empty() {
        println!("No enumerations found.");
        return;
    }

    if skip_original && enumerations.len() <= 1 {
        println!("Skipped: only original enumeration exists.");
        return;
    }

    if let Some(enum_idx) = opt.index {
        if enum_idx >= enumerations.len() {
            eprintln!(
                "Error: Requested enumeration index {} is out of bounds (max {}).",
                enum_idx,
                enumerations.len() - 1
            );
            return;
        }
        let out_str = enumerator::apply_assignment(&module, &enumerations[enum_idx]);
        println!("{}", out_str);
    } else {
        println!(
            "// Found {} holes, {} valid enumerations.",
            holes,
            enumerations.len()
        );
        if let Some(orig) = original_assignment_idx {
            println!("// Original assignment is at index {}.", orig);
        }

        for (i, assigns) in enumerations.iter().enumerate() {
            let out_str = enumerator::apply_assignment(&module, assigns);
            println!("// === Enumeration {} ===", i);
            println!("{}", out_str);
        }
    }
}

fn do_healthcheck(wgslsmith_exe: &Path, server: &str, configs: &[ConfigId]) {
    let mut cmd = process::Command::new(wgslsmith_exe);
    cmd.arg("remote").arg(server).arg("run");

    for config in configs {
        cmd.arg("-c").arg(config.to_string());
    }

    cmd.arg("-");

    cmd.stdin(process::Stdio::piped());
    cmd.stdout(process::Stdio::piped());
    cmd.stderr(process::Stdio::piped());

    let output = match cmd.spawn() {
        Ok(mut child) => {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(b"@compute\n@workgroup_size(1)\nfn main() {}");
            }

            child.wait_with_output()
        }
        Err(e) => Err(e),
    };

    match output {
        Ok(out) => {
            if !out.status.success() {
                println!("\nHealthcheck failed. Terminating.");
                print_stats();
                std::process::exit(1);
            }
        }
        Err(e) => {
            println!("\nHealthcheck command failed: {}. Terminating.", e);
            print_stats();
            std::process::exit(1);
        }
    }
}

fn check_extension_support(
    wgslsmith_exe: &Path,
    server: Option<&String>,
    configs: &[ConfigId],
) -> std::collections::HashMap<ast::EnableExtension, std::collections::HashSet<ConfigId>> {
    let mut support_map = std::collections::HashMap::new();

    println!("\nChecking extension support for configs...");

    for ext in <ast::EnableExtension as strum::IntoEnumIterator>::iter() {
        let mut supported_configs = std::collections::HashSet::new();
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

fn run_process_dir(opt: DirOptions, skip_original: bool) {
    let wgslsmith_exe = std::env::current_exe().expect("Failed to get current executable path");

    if let Some(server) = &opt.server {
        do_healthcheck(&wgslsmith_exe, server, &opt.configs);
    }

    let support_map = if opt.skip_ext_filter {
        std::collections::HashMap::new()
    } else {
        check_extension_support(&wgslsmith_exe, opt.server.as_ref(), &opt.configs)
    };

    let mut effective_start_index = opt.start_index;

    let log_to_file = opt.log_to_file || opt.append_dir.is_some();

    let (out_dir_opt, append) = if let Some(dir) = &opt.append_dir {
        let last_idx = load_stats(dir);

        if effective_start_index.is_none() && last_idx > 0 {
            effective_start_index = Some(last_idx);
        }

        (Some(dir.clone()), true)
    } else if log_to_file {
        let offset = *UTC_OFFSET.get().unwrap_or(&UtcOffset::UTC);
        let now = OffsetDateTime::now_utc().to_offset(offset);
        let format =
            format_description::parse("spe-[year]-[month]-[day]-[hour]-[minute]-[second]").unwrap();
        let dir_name = now.format(&format).unwrap();
        (Some(PathBuf::from(dir_name)), false)
    } else {
        (None, false)
    };

    if let Some(out_dir) = &out_dir_opt {
        fs::create_dir_all(out_dir).unwrap();
        let _ = OUT_DIR.set(out_dir.clone());
    }

    let (mut skipped_log, mut failures_log) = get_logs(log_to_file, out_dir_opt.as_deref(), append);

    let entries: Vec<_> = WalkDir::new(&opt.directory)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| {
            let p = e.path();
            p.extension().is_some_and(|ext| ext == "wgsl")
                && !p.to_string_lossy().ends_with(".expected.wgsl")
        })
        .collect();

    let total_files = entries.len();
    let mut shaders_processed = 0;

    for (file_idx, entry) in entries.into_iter().enumerate() {
        let path = entry.path();
        let file_num = file_idx + 1;

        if let Some(start_index) = effective_start_index {
            if file_num < start_index {
                continue;
            }
        }

        LAST_INDEX.store(file_num, Ordering::SeqCst);
        DIRTY_STATS.store(true, Ordering::SeqCst);

        process_shader_file(
            path,
            file_num,
            Some(total_files),
            &opt,
            skip_original,
            &wgslsmith_exe,
            out_dir_opt.as_deref(),
            &mut *skipped_log,
            &mut *failures_log,
            &support_map,
        );

        shaders_processed += 1;
        if shaders_processed % 50 == 0 {
            write_stats_json();

            if let Some(server) = &opt.server {
                do_healthcheck(&wgslsmith_exe, server, &opt.configs);
            }
        }
    }

    print_stats();
}

fn generate_inputs_if_needed(module: &ast::Module) -> Option<String> {
    use ast::{StorageClass, VarQualifier};
    use rand::Rng;
    let mut init_data = std::collections::BTreeMap::new();
    let mut rng = rand::thread_rng();

    for var in &module.vars {
        if let Some(VarQualifier { storage_class, .. }) = &var.qualifier {
            if *storage_class != StorageClass::Uniform && *storage_class != StorageClass::Storage {
                continue;
            }

            if let Ok(type_desc) = common::Type::try_from(&var.data_type) {
                if let (Some(group), Some(binding)) = (var.group_index(), var.binding_index()) {
                    let size = type_desc.buffer_size();
                    let data: Vec<u8> = (0..size).map(|_| rng.gen()).collect();
                    init_data.insert(format!("{group}:{binding}"), data);
                }
            }
        }
    }

    if !init_data.is_empty() {
        if let Ok(json) = serde_json::to_string(&init_data) {
            return Some(json);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn process_shader_file(
    path: &Path,
    file_num: usize,
    total_files: Option<usize>,
    opt: &DirOptions,
    skip_original: bool,
    wgslsmith_exe: &Path,
    out_dir: Option<&Path>,
    skipped_log: &mut dyn Write,
    failures_log: &mut dyn Write,
    support_map: &std::collections::HashMap<
        ast::EnableExtension,
        std::collections::HashSet<ConfigId>,
    >,
) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut input_buffers = fs::read_to_string(path.with_extension("in.json")).ok();

    let module = match std::panic::catch_unwind(|| parser::parse(&content)) {
        Ok(m) => m,
        Err(_) => {
            writeln!(
                failures_log,
                "[{}] [{}] Parse panic on: {}",
                current_timestamp(),
                file_num,
                path.display()
            )
            .unwrap();
            STAT_FAILED_PARSE.fetch_add(1, Ordering::SeqCst);
            return;
        }
    };

    if input_buffers.is_none() {
        input_buffers = generate_inputs_if_needed(&module);
    }

    let stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    process_shader_core(
        &module,
        input_buffers,
        path.display().to_string(),
        stem,
        file_num,
        total_files,
        opt,
        skip_original,
        wgslsmith_exe,
        out_dir,
        skipped_log,
        failures_log,
        support_map,
        100, // max_enumerations for process-dir
    );
}

#[allow(clippy::too_many_arguments)]
fn process_shader_core(
    module: &ast::Module,
    input_buffers: Option<String>,
    path_display: String,
    stem: String,
    file_num: usize,
    total_files: Option<usize>,
    opt: &DirOptions,
    skip_original: bool,
    wgslsmith_exe: &Path,
    out_dir: Option<&Path>,
    skipped_log: &mut dyn Write,
    failures_log: &mut dyn Write,
    support_map: &std::collections::HashMap<
        ast::EnableExtension,
        std::collections::HashSet<ConfigId>,
    >,
    max_enumerations: usize,
) {
    let progress_prefix = if let Some(total) = total_files {
        format!("[{}/{}] ", file_num, total)
    } else {
        "".to_string()
    };

    let mut current_configs = opt.configs.clone();

    for ext in &module.enables {
        if let Some(supported) = support_map.get(ext) {
            let original_len = current_configs.len();
            current_configs.retain(|c| supported.contains(c));

            if current_configs.len() < original_len {
                writeln!(
                    skipped_log,
                    "[{}] [{}] Filtered configs for: {} (uses {})",
                    current_timestamp(),
                    file_num,
                    path_display,
                    ext
                )
                .unwrap();
            }
        }
    }

    if current_configs.is_empty() {
        writeln!(
            skipped_log,
            "[{}] [{}] Skipping: {} (uses extensions not supported by any config)",
            current_timestamp(),
            file_num,
            path_display
        )
        .unwrap();
        println!(
            "[{}] {}Skipped: {} (uses extensions not supported by any config)",
            current_timestamp(),
            progress_prefix,
            path_display
        );
        return;
    }

    let (holes, mut enumerations, original_assignment_idx) = {
        let est = enumerator::estimate_enumerations(module);
        let limit = if est > 100_000 {
            writeln!(
                skipped_log,
                "[{}] [{}] Warning: {} (estimated {} bounds, > 100,000). Limiting search to 2000 variants.",
                current_timestamp(),
                file_num,
                path_display,
                est
            )
            .unwrap();
            println!(
                "[{}] {}Large enumeration space: {} (estimated {} bounds). Limiting search to 2000 variants.",
                current_timestamp(),
                progress_prefix,
                path_display,
                est
            );
            Some(2000)
        } else {
            None
        };
        match std::panic::catch_unwind(|| enumerator::get_enumerations(module, limit)) {
            Ok(res) => res,
            Err(_) => {
                writeln!(
                    failures_log,
                    "[{}] [{}] Enumerate panic on: {}",
                    current_timestamp(),
                    file_num,
                    path_display
                )
                .unwrap();
                STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                return;
            }
        }
    };

    if original_assignment_idx.is_none() {
        writeln!(
            failures_log,
            "[{}] [{}] No original assignment found for: {}",
            current_timestamp(),
            file_num,
            path_display
        )
        .unwrap();
        STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
        return;
    }

    let original_assignment = original_assignment_idx.map(|idx| enumerations[idx].clone());

    if skip_original && enumerations.len() <= 1 {
        writeln!(
            skipped_log,
            "[{}] [{}] Skipping: {} (only original enumeration exists)",
            current_timestamp(),
            file_num,
            path_display
        )
        .unwrap();
        println!(
            "[{}] {}Skipped: {} (only original enumeration exists)",
            current_timestamp(),
            progress_prefix,
            path_display
        );
        STAT_SKIPPED_ONLY_ORIGINAL.fetch_add(1, Ordering::SeqCst);
        return;
    }

    // Rearrange so original is always at the beginning
    if let Some(orig) = &original_assignment {
        enumerations.retain(|e| e != orig);
        enumerations.insert(0, orig.clone());
    }

    if enumerations.len() > max_enumerations {
        println!(
            "[{}] {}Downsampling: {} ({} enumerations -> {} randomly sampled, {} holes)",
            current_timestamp(),
            progress_prefix,
            path_display,
            enumerations.len(),
            max_enumerations,
            holes
        );
        STAT_SUBSAMPLED.fetch_add(1, Ordering::SeqCst);

        let mut rng = rand::thread_rng();
        // Ensure original isn't wiped out during truncation
        if original_assignment.is_some() {
            let first = enumerations.remove(0);
            enumerations.shuffle(&mut rng);
            enumerations.truncate(max_enumerations.saturating_sub(1));
            enumerations.insert(0, first);
        } else {
            enumerations.shuffle(&mut rng);
            enumerations.truncate(max_enumerations);
        }
    } else {
        println!(
            "[{}] {}Processing: {} ({} enumerations, {} holes)",
            current_timestamp(),
            progress_prefix,
            path_display,
            enumerations.len(),
            holes
        );
    }

    let mut failed_count = 0;
    let mut has_success = false;
    let mut failures_to_save = Vec::new();

    for (i, assigns) in enumerations.iter().enumerate() {
        let is_original = Some(assigns) == original_assignment.as_ref();
        let case_str = if is_original {
            format!("(case {i} (original))")
        } else {
            format!("(case {i})")
        };

        let out_str =
            match std::panic::catch_unwind(|| enumerator::apply_assignment(module, assigns)) {
                Ok(s) => s,
                Err(_) => {
                    writeln!(
                        failures_log,
                        "[{}] [{}] Apply assignment panic on: {} {case_str}",
                        current_timestamp(),
                        file_num,
                        path_display
                    )
                    .unwrap();
                    failed_count += 1;
                    if is_original {
                        STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                        println!(
                        "[{}] {}Skipped variants for {} (original panicked on apply_assignment)",
                        current_timestamp(),
                        progress_prefix,
                        path_display
                    );
                        break;
                    }
                    if failed_count >= 10 {
                        println!(
                            "[{}] {}Skipped remaining enumerations for {} (>= 10 failures)",
                            current_timestamp(),
                            progress_prefix,
                            path_display
                        );
                        break;
                    }
                    continue;
                }
            };

        let mut current_src = out_str.clone();

        let mut recond_cmd = process::Command::new(wgslsmith_exe);
        recond_cmd.arg("recondition").arg("-");

        recond_cmd
            .stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped());

        let recond_output = recond_cmd.spawn().and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(current_src.as_bytes())?;
            }
            child.wait_with_output()
        });

        match recond_output {
            Ok(out) => {
                if out.status.success() {
                    current_src = String::from_utf8_lossy(&out.stdout).into_owned();
                } else {
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    writeln!(
                        failures_log,
                        "[{}] [{}] Recondition failed for: {} {}\nStderr: {}",
                        current_timestamp(),
                        file_num,
                        path_display,
                        case_str,
                        stderr
                    )
                    .unwrap();

                    if is_original {
                        STAT_FAILED_RECONDITION.fetch_add(1, Ordering::SeqCst);
                        println!(
                            "[{}] {}Skipped variants for {} (original failed recondition)",
                            current_timestamp(),
                            progress_prefix,
                            path_display
                        );
                        break;
                    }
                }
            }
            Err(e) => {
                writeln!(
                    failures_log,
                    "[{}] [{}] Failed to execute wslinux recondition for: {} {}\nError: {}",
                    current_timestamp(),
                    file_num,
                    path_display,
                    case_str,
                    e
                )
                .unwrap();

                if is_original {
                    STAT_FAILED_RECONDITION.fetch_add(1, Ordering::SeqCst);
                    println!(
                        "[{}] {}Skipped variants for {} (original failed to execute recondition)",
                        current_timestamp(),
                        progress_prefix,
                        path_display
                    );
                    break;
                }
            }
        }

        if opt.msl_validate {
            let msl_tint_ok = run_compile(
                wgslsmith_exe,
                &current_src,
                "msl",
                "tint",
                failures_log,
                &format!("[{}] for {} {}", file_num, path_display, case_str),
            );

            let mut msl_naga_ok = true;
            if !module.enables.contains(&ast::EnableExtension::Subgroups) {
                msl_naga_ok = run_compile(
                    wgslsmith_exe,
                    &current_src,
                    "msl",
                    "naga",
                    failures_log,
                    &format!("[{}] for {} {}", file_num, path_display, case_str),
                );
            }

            if !msl_tint_ok || !msl_naga_ok {
                failed_count += 1;

                if is_original {
                    STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                    println!(
                        "[{}] {}Skipped variants for {} (original failed msl_validate)",
                        current_timestamp(),
                        progress_prefix,
                        path_display
                    );
                    break;
                }
            }
        }

        let mut cmd = process::Command::new(wgslsmith_exe);

        if let Some(server) = &opt.server {
            cmd.arg("remote").arg(server);
        }

        cmd.arg("run");

        for config in &current_configs {
            cmd.arg("-c").arg(config.to_string());
        }

        cmd.arg("-j");

        if let Some(j) = opt.parallelism {
            cmd.arg(j.to_string());
        } else {
            cmd.arg("2");
        }

        if opt.use_daemon {
            cmd.arg("--use-daemon");
        }

        cmd.arg("--print-consensus");

        cmd.arg("-");

        if let Some(input_buffers) = &input_buffers {
            cmd.arg(input_buffers);
        }

        cmd.stdin(process::Stdio::piped())
            .stdout(process::Stdio::piped())
            .stderr(process::Stdio::piped());

        let output = cmd.spawn().and_then(|mut child| {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(current_src.as_bytes())?;
            }
            child.wait_with_output()
        });

        match output {
            Ok(out) => {
                let stdout_str = String::from_utf8_lossy(&out.stdout);
                let stderr_str = String::from_utf8_lossy(&out.stderr);

                let mut combined_output = String::new();
                let mut consensus_json = String::new();

                for line in stdout_str.lines() {
                    if let Some(json_content) = line.strip_prefix("output-consensus: ") {
                        consensus_json = json_content.to_string();
                    } else {
                        combined_output.push_str(line);
                        combined_output.push('\n');
                    }
                }
                for line in stderr_str.lines() {
                    combined_output.push_str(line);
                    combined_output.push('\n');
                }

                let combined_bytes = combined_output.replace('\0', "").into_bytes();

                if !out.status.success() {
                    writeln!(
                        failures_log,
                        "[{}] [{}] Failed validation for: {} {case_str}\nStdout: {}\nStderr: {}",
                        current_timestamp(),
                        file_num,
                        path_display,
                        stdout_str,
                        stderr_str
                    )
                    .unwrap();
                    failed_count += 1;

                    let kind = if out.status.code() == Some(1) {
                        "mismatch"
                    } else {
                        "crash"
                    };

                    let recond_src = current_src.clone();
                    failures_to_save.push((
                        i,
                        kind.to_string(),
                        consensus_json.into_bytes(),
                        combined_bytes,
                        out_str.clone(),
                        recond_src,
                        is_original,
                    ));

                    if is_original {
                        STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                        println!(
                            "[{}] {}Skipped variants for {} (original failed validation)",
                            current_timestamp(),
                            progress_prefix,
                            path_display
                        );
                        break;
                    }
                } else {
                    if is_original {
                        STAT_RUN_SUCCESS.fetch_add(1, Ordering::SeqCst);
                    }
                    has_success = true;
                }
            }
            Err(e) => {
                writeln!(
                    failures_log,
                    "[{}] [{}] Failed to run wgslsmith for: {} {case_str}\nError: {}",
                    current_timestamp(),
                    file_num,
                    path_display,
                    e
                )
                .unwrap();
                failed_count += 1;

                let recond_src = current_src.clone();
                failures_to_save.push((
                    i,
                    "crash".to_string(),
                    Vec::new(),
                    format!("Error: {}", e).into_bytes(),
                    out_str.clone(),
                    recond_src,
                    is_original,
                ));

                if is_original {
                    STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                    println!(
                        "[{}] {}Skipped variants for {} (original failed execution)",
                        current_timestamp(),
                        progress_prefix,
                        path_display
                    );
                    break;
                }
            }
        }

        if failed_count >= 10 {
            println!(
                "[{}] {}Skipped remaining enumerations for {} (>= 10 failures)",
                current_timestamp(),
                progress_prefix,
                path_display
            );
            break;
        }
    }

    if !failures_to_save.is_empty() {
        if let Some(out_dir) = out_dir {
            for (i, kind, consensus, combined, src, recond_src, is_original) in failures_to_save {
                if !is_original && !has_success {
                    continue;
                }

                let base_out = if is_original {
                    out_dir.join("original-out")
                } else {
                    out_dir.join("out")
                };

                let failure_out_dir = base_out.join(format!("{}_{}-{}-{kind}", stem, file_num, i));
                std::fs::create_dir_all(&failure_out_dir).unwrap();

                std::fs::write(failure_out_dir.join("shader.wgsl"), src).unwrap();
                std::fs::write(failure_out_dir.join("reconditioned.wgsl"), recond_src).unwrap();

                if let Some(in_bufs) = &input_buffers {
                    std::fs::write(failure_out_dir.join("inputs.json"), in_bufs).unwrap();
                }

                let configs_str = opt
                    .configs
                    .iter()
                    .map(|c| format!("\"{}\"", c))
                    .collect::<Vec<_>>()
                    .join(", ");

                let name = opt.server.clone().unwrap_or_else(|| {
                    std::env::var("HOSTNAME")
                        .or_else(|_| std::env::var("COMPUTERNAME"))
                        .unwrap_or_else(|_| "unknown-machine".to_string())
                });
                let info = format!(
                    "{{\n  \"configs\": [{}],\n  \"kind\": \"{}\",\n  \"name\": \"{name}\",\n  \"flags\": []\n}}",
                    configs_str, kind
                );
                std::fs::write(failure_out_dir.join("info.json"), info).unwrap();

                std::fs::write(failure_out_dir.join("stderr.txt"), combined).unwrap();

                if kind == "mismatch" && !consensus.is_empty() {
                    std::fs::write(failure_out_dir.join("consensus.json"), consensus).unwrap();
                }
            }
        }
    }
}
