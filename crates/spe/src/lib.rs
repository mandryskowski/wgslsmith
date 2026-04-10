pub mod enumerator;

use clap::{Parser, Subcommand};
use harness_types::{ConfigId, Implementation};
use rand::seq::SliceRandom;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;
use time::{format_description, OffsetDateTime, UtcOffset};
use walkdir::WalkDir;

static ENUMERATED_NO_ISSUE: AtomicUsize = AtomicUsize::new(0);
static FAILED_PARSE: AtomicUsize = AtomicUsize::new(0);
static FAILED_RUN: AtomicUsize = AtomicUsize::new(0);
static LAST_INDEX: AtomicUsize = AtomicUsize::new(0);

static OUT_DIR: OnceLock<PathBuf> = OnceLock::new();
static UTC_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

fn print_stats() {
    let enumerated = ENUMERATED_NO_ISSUE.load(Ordering::SeqCst);
    let parse_failed = FAILED_PARSE.load(Ordering::SeqCst);
    let run_failed = FAILED_RUN.load(Ordering::SeqCst);
    let last_idx = LAST_INDEX.load(Ordering::SeqCst);

    let args: Vec<String> = std::env::args().collect();

    println!("\n=== SPE Execution Statistics ===");
    println!(
        "- Command ran:                                {}",
        args.join(" ")
    );
    println!(
        "- Number of shaders enumerated with no issue: {}",
        enumerated
    );
    println!(
        "- Number of shaders failed to parse:          {}",
        parse_failed
    );
    println!(
        "- Number of shaders failed while running:     {}",
        run_failed
    );
    println!("- Last handled shader index:                  {}", last_idx);
    println!("================================\n");

    if let Some(out_dir) = OUT_DIR.get() {
        let args_json = args
            .iter()
            .map(|a| format!("\"{}\"", a.replace('\\', "\\\\").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(", ");

        let json = format!(
            "{{\n  \"args\": [{}],\n  \"enumerated_no_issue\": {},\n  \"failed_parse\": {},\n  \"failed_run\": {},\n  \"last_handled_index\": {}\n}}",
            args_json, enumerated, parse_failed, run_failed, last_idx
        );
        let _ = fs::write(out_dir.join("stats.json"), json);
    }
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
        ENUMERATED_NO_ISSUE.store(parse_val("\"enumerated_no_issue\""), Ordering::SeqCst);
        FAILED_PARSE.store(parse_val("\"failed_parse\""), Ordering::SeqCst);
        FAILED_RUN.store(parse_val("\"failed_run\""), Ordering::SeqCst);

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
    ProcessDir(ProcessDirOptions),
}

#[derive(Parser, Debug)]
pub struct EnumerateOptions {
    /// Path to the WGSL shader
    pub shader_path: PathBuf,

    /// Run/print a specific enumeration of a shader
    #[clap(long)]
    pub enumeration: Option<usize>,
}

#[derive(Parser, Debug)]
pub struct ProcessDirOptions {
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
    server: Option<String>,

    #[clap(long, action, default_value = "false")]
    pub msl_validate: bool,

    /// Run fallback compile validations when no compute entrypoint is found
    #[clap(long, action, default_value = "false")]
    pub compile_validate: bool,
}

fn run_compile(
    wgslsmith_exe: &Path,
    file: &Path,
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
        .arg(file);

    match cmd.output() {
        Ok(out) => {
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                writeln!(
                    failures_log,
                    "Failed compile (--backend {} --compiler {}) {}\nStdout: {}\nStderr: {}",
                    backend, compiler, context, stdout, stderr
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
                "Failed to execute wslinux compile (--backend {} --compiler {}) {}\nError: {}",
                backend, compiler, context, e
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
    }
}

fn get_logs(log_to_file: bool, out_dir: &Path, append: bool) -> (Box<dyn Write>, Box<dyn Write>) {
    let skipped_log: Box<dyn Write> = if log_to_file {
        Box::new(
            fs::OpenOptions::new()
                .create(true)
                .append(append)
                .write(true)
                .truncate(!append)
                .open(out_dir.join("skipped.log"))
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
                .open(out_dir.join("failures.log"))
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

    let mut module = match std::panic::catch_unwind(|| parser::parse(&content)) {
        Ok(m) => m,
        Err(_) => {
            eprintln!("Parse panic on: {}", opt.shader_path.display());
            return;
        }
    };

    if !enumerator::filter_module(&mut module) {
        eprintln!("No compute entrypoint: {}", opt.shader_path.display());
        return;
    }

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

    if let Some(enum_idx) = opt.enumeration {
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

fn run_process_dir(opt: ProcessDirOptions, skip_original: bool) {
    let wgslsmith_exe = std::env::current_exe().expect("Failed to get current executable path");

    let mut effective_start_index = opt.start_index;

    let (out_dir, append) = if let Some(dir) = &opt.append_dir {
        let last_idx = load_stats(dir);

        if effective_start_index.is_none() && last_idx > 0 {
            effective_start_index = Some(last_idx);
        }

        (dir.clone(), true)
    } else {
        let offset = *UTC_OFFSET.get().unwrap_or(&UtcOffset::UTC);
        let now = OffsetDateTime::now_utc().to_offset(offset);
        let format =
            format_description::parse("spe-[year]-[month]-[day]-[hour]-[minute]-[second]").unwrap();
        let dir_name = now.format(&format).unwrap();
        (PathBuf::from(dir_name), false)
    };

    fs::create_dir_all(&out_dir).unwrap();
    let _ = OUT_DIR.set(out_dir.clone());

    let log_to_file = opt.log_to_file || opt.append_dir.is_some();
    let (mut skipped_log, mut failures_log) = get_logs(log_to_file, &out_dir, append);

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

    for (file_idx, entry) in entries.into_iter().enumerate() {
        let path = entry.path();
        let file_num = file_idx + 1;

        if let Some(start_index) = effective_start_index {
            if file_num < start_index {
                continue;
            }
        }

        LAST_INDEX.store(file_num, Ordering::SeqCst);

        process_shader(
            path,
            file_num,
            Some(total_files),
            &opt,
            skip_original,
            &wgslsmith_exe,
            &out_dir,
            &mut *skipped_log,
            &mut *failures_log,
        );
    }

    print_stats();
}

#[allow(clippy::too_many_arguments)]
fn process_shader(
    path: &Path,
    file_num: usize,
    total_files: Option<usize>,
    opt: &ProcessDirOptions,
    skip_original: bool,
    wgslsmith_exe: &Path,
    out_dir: &Path,
    skipped_log: &mut dyn Write,
    failures_log: &mut dyn Write,
) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let input_buffers = fs::read_to_string(path.with_extension("in.json")).ok();

    let mut current_configs = opt.configs.clone();

    let progress_prefix = if let Some(total) = total_files {
        format!("[{}/{}] ", file_num, total)
    } else {
        "".to_string()
    };

    if content.contains("subgroup") && !current_configs.is_empty() {
        current_configs.retain(|c| c.implementation != Implementation::Wgpu);
        if current_configs.is_empty() {
            writeln!(
                skipped_log,
                "Skipping: {} (uses subgroups, no valid configs left)",
                path.display()
            )
            .unwrap();
            println!(
                "{}Skipped: {} (uses subgroups, no valid configs left)",
                progress_prefix,
                path.display()
            );
            return;
        }
    }

    if content.contains("f16") {
        let original_len = current_configs.len();
        current_configs.retain(|c| {
            let config_str = c.to_string();
            config_str != "dawn:vk:8593"
        });

        if current_configs.len() < original_len {
            writeln!(
                skipped_log,
                "Filtering dawn:vk:8593 from: {} (uses f16)",
                path.display()
            )
            .unwrap();
        }
    }

    let mut module = match std::panic::catch_unwind(|| parser::parse(&content)) {
        Ok(m) => m,
        Err(_) => {
            writeln!(failures_log, "Parse panic on: {}", path.display()).unwrap();
            FAILED_PARSE.fetch_add(1, Ordering::SeqCst);
            return;
        }
    };

    if !enumerator::filter_module(&mut module) {
        writeln!(
            skipped_log,
            "Skipping: {} (no compute entrypoint left after filtering)",
            path.display()
        )
        .unwrap();

        if !opt.compile_validate {
            return;
        }

        println!(
            "{}No compute entrypoint: {}. Running compile validations.",
            progress_prefix,
            path.display()
        );

        for backend in &["hlsl", "spirv", "msl"] {
            for compiler in &["tint", "naga"] {
                run_compile(
                    wgslsmith_exe,
                    path,
                    backend,
                    compiler,
                    failures_log,
                    &format!("(no entrypoint for {})", path.display()),
                );
            }
        }
        return;
    }

    let (holes, mut enumerations, original_assignment_idx) = {
        let est = enumerator::estimate_enumerations(&module);
        let limit = if est > 100_000 {
            writeln!(
                skipped_log,
                "Warning: {} (estimated {} bounds, > 100,000). Limiting search to 2000 variants.",
                path.display(),
                est
            )
            .unwrap();
            println!(
                "{}Large enumeration space: {} (estimated {} bounds). Limiting search to 2000 variants.",
                progress_prefix,
                path.display(),
                est
            );
            Some(2000)
        } else {
            None
        };
        match std::panic::catch_unwind(|| enumerator::get_enumerations(&module, limit)) {
            Ok(res) => res,
            Err(_) => {
                writeln!(failures_log, "Enumerate panic on: {}", path.display()).unwrap();
                FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                return;
            }
        }
    };

    if original_assignment_idx.is_none() {
        writeln!(
            failures_log,
            "No original assignment found for: {}",
            path.display()
        )
        .unwrap();
        FAILED_RUN.fetch_add(1, Ordering::SeqCst);
        return;
    }

    let original_assignment = original_assignment_idx.map(|idx| enumerations[idx].clone());

    if skip_original && enumerations.len() <= 1 {
        writeln!(
            skipped_log,
            "Skipping: {} (only original enumeration exists)",
            path.display()
        )
        .unwrap();
        println!(
            "{}Skipped: {} (only original enumeration exists)",
            progress_prefix,
            path.display()
        );
        return;
    }

    // Rearrange so original is always at the beginning
    if let Some(orig) = &original_assignment {
        enumerations.retain(|e| e != orig);
        enumerations.insert(0, orig.clone());
    }

    if enumerations.len() > 100 {
        println!(
            "{}Downsampling: {} ({} enumerations -> 100 randomly sampled, {} holes)",
            progress_prefix,
            path.display(),
            enumerations.len(),
            holes
        );

        let mut rng = rand::thread_rng();
        // Ensure original isn't wiped out during truncation
        if original_assignment.is_some() {
            let first = enumerations.remove(0);
            enumerations.shuffle(&mut rng);
            enumerations.truncate(99);
            enumerations.insert(0, first);
        } else {
            enumerations.shuffle(&mut rng);
            enumerations.truncate(100);
        }
    } else {
        println!(
            "{}Processing: {} ({} enumerations, {} holes)",
            progress_prefix,
            path.display(),
            enumerations.len(),
            holes
        );
    }

    let mut failed_count = 0;
    let mut shader_has_runtime_failure = false;
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
            match std::panic::catch_unwind(|| enumerator::apply_assignment(&module, assigns)) {
                Ok(s) => s,
                Err(_) => {
                    writeln!(
                        failures_log,
                        "Apply assignment panic on: {} {case_str}",
                        path.display()
                    )
                    .unwrap();
                    failed_count += 1;
                    shader_has_runtime_failure = true;
                    if is_original {
                        println!(
                            "{}Skipped variants for {} (original panicked on apply_assignment)",
                            progress_prefix,
                            path.display()
                        );
                        break;
                    }
                    if failed_count >= 10 {
                        println!(
                            "{}Skipped remaining enumerations for {} (>= 10 failures)",
                            progress_prefix,
                            path.display()
                        );
                        break;
                    }
                    continue;
                }
            };

        let tmp_path = std::env::temp_dir().join(format!(
            "shader_{}_{}_{}.wgsl",
            std::process::id(),
            file_num,
            i
        ));

        fs::write(&tmp_path, &out_str).expect("Failed to write temporary shader file");

        let mut recond_cmd = process::Command::new(wgslsmith_exe);
        recond_cmd.arg("recondition").arg(&tmp_path);

        match recond_cmd.output() {
            Ok(out) => {
                if out.status.success() {
                    let reconditioned_src = String::from_utf8_lossy(&out.stdout);
                    fs::write(&tmp_path, reconditioned_src.as_ref())
                        .expect("Failed to write reconditioned temporary shader file");
                } else {
                    shader_has_runtime_failure = true;
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    writeln!(
                        failures_log,
                        "Recondition failed for: {} {}\nStderr: {}",
                        path.display(),
                        case_str,
                        stderr
                    )
                    .unwrap();

                    if is_original {
                        fs::remove_file(&tmp_path).ok();
                        println!(
                            "{}Skipped variants for {} (original failed recondition)",
                            progress_prefix,
                            path.display()
                        );
                        break;
                    }
                }
            }
            Err(e) => {
                shader_has_runtime_failure = true;
                writeln!(
                    failures_log,
                    "Failed to execute wslinux recondition for: {} {}\nError: {}",
                    path.display(),
                    case_str,
                    e
                )
                .unwrap();

                if is_original {
                    fs::remove_file(&tmp_path).ok();
                    println!(
                        "{}Skipped variants for {} (original failed to execute recondition)",
                        progress_prefix,
                        path.display()
                    );
                    break;
                }
            }
        }

        if opt.msl_validate {
            let msl_tint_ok = run_compile(
                wgslsmith_exe,
                &tmp_path,
                "msl",
                "tint",
                failures_log,
                &format!("for {} {}", path.display(), case_str),
            );

            let mut msl_naga_ok = true;
            if !content.contains("subgroup") {
                msl_naga_ok = run_compile(
                    wgslsmith_exe,
                    &tmp_path,
                    "msl",
                    "naga",
                    failures_log,
                    &format!("for {} {}", path.display(), case_str),
                );
            }

            if !msl_tint_ok || !msl_naga_ok {
                failed_count += 1;
                shader_has_runtime_failure = true;

                if is_original {
                    fs::remove_file(&tmp_path).ok();
                    println!(
                        "{}Skipped variants for {} (original failed msl_validate)",
                        progress_prefix,
                        path.display()
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

        cmd.arg(&tmp_path);

        if let Some(input_buffers) = &input_buffers {
            cmd.arg(input_buffers);
        }

        let output = cmd.output();
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
                        "Failed validation for: {} {case_str}\nStdout: {}\nStderr: {}",
                        path.display(),
                        stdout_str,
                        stderr_str
                    )
                    .unwrap();
                    failed_count += 1;
                    shader_has_runtime_failure = true;

                    let kind = if out.status.code() == Some(1) {
                        "mismatch"
                    } else {
                        "crash"
                    };

                    let recond_src = fs::read_to_string(&tmp_path).unwrap_or_default();
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
                        fs::remove_file(&tmp_path).ok();
                        println!(
                            "{}Skipped variants for {} (original failed validation)",
                            progress_prefix,
                            path.display()
                        );
                        break;
                    }
                } else {
                    has_success = true;
                }
            }
            Err(e) => {
                writeln!(
                    failures_log,
                    "Failed to run wgslsmith for: {} {case_str}\nError: {}",
                    path.display(),
                    e
                )
                .unwrap();
                failed_count += 1;
                shader_has_runtime_failure = true;

                let recond_src = fs::read_to_string(&tmp_path).unwrap_or_default();
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
                    fs::remove_file(&tmp_path).ok();
                    println!(
                        "{}Skipped variants for {} (original failed execution)",
                        progress_prefix,
                        path.display()
                    );
                    break;
                }
            }
        }

        fs::remove_file(&tmp_path).ok();

        if failed_count >= 10 {
            println!(
                "{}Skipped remaining enumerations for {} (>= 10 failures)",
                progress_prefix,
                path.display()
            );
            break;
        }
    }

    if !failures_to_save.is_empty() {
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        for (i, kind, consensus, combined, src, recond_src, is_original) in failures_to_save {
            // Only output variant failure files if we had at least one success recorded
            // from earlier processing (original succeeded, variants failed).
            if !is_original && !has_success {
                continue;
            }

            let base_out = if is_original {
                out_dir.join("original-out")
            } else {
                out_dir.join("out")
            };

            let failure_out_dir = base_out.join(format!("{}_{}-{kind}", stem, i));
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

    if shader_has_runtime_failure {
        FAILED_RUN.fetch_add(1, Ordering::SeqCst);
    } else {
        ENUMERATED_NO_ISSUE.fetch_add(1, Ordering::SeqCst);
    }
}
