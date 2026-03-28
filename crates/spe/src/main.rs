use clap::Parser;
use harness_types::{ConfigId, Implementation};
use rand::seq::SliceRandom;
use spe::apply_assignment;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use walkdir::WalkDir;

static ENUMERATED_NO_ISSUE: AtomicUsize = AtomicUsize::new(0);
static FAILED_PARSE: AtomicUsize = AtomicUsize::new(0);
static FAILED_RUN: AtomicUsize = AtomicUsize::new(0);

fn print_stats() {
    println!("\n=== SPE Execution Statistics ===");
    println!(
        "- Number of shaders enumerated with no issue: {}",
        ENUMERATED_NO_ISSUE.load(Ordering::SeqCst)
    );
    println!(
        "- Number of shaders failed to parse:          {}",
        FAILED_PARSE.load(Ordering::SeqCst)
    );
    println!(
        "- Number of shaders failed while running:     {}",
        FAILED_RUN.load(Ordering::SeqCst)
    );
    println!("================================\n");
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to scan for WGSL shaders
    directory: PathBuf,
    /// Path to wslinux executable (optional)
    #[arg(long)]
    wslinux: Option<PathBuf>,

    #[clap(long, action, default_value = "false")]
    pub log_to_file: bool,

    /// Control parallelism passed to run
    #[clap(short = 'j', long)]
    pub parallelism: Option<usize>,

    #[clap(long, action, default_value = "false")]
    pub use_daemon: bool,

    /// Print out shaders instead of running them
    #[clap(long, action, default_value = "false")]
    pub print_only: bool,

    /// Run/print a specific enumeration of a shader
    #[clap(long)]
    pub enumeration: Option<usize>,

    /// Configurations to run (e.g., wgpu:dx12:5592)
    #[clap(short, long = "config", action)]
    pub configs: Vec<ConfigId>,

    #[clap(long, action, default_value = "false")]
    pub msl_validate: bool,

    #[clap(long)]
    pub start_index: Option<usize>,
}

fn run_compile(
    wslinux: &Path,
    file: &Path,
    backend: &str,
    compiler: &str,
    failures_log: &mut dyn Write,
    context: &str,
) -> bool {
    let mut cmd = Command::new(wslinux);
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

fn main() {
    if let Err(e) = ctrlc::set_handler(move || {
        println!("\nProcess interrupted by user.");
        print_stats();
        std::process::exit(0);
    }) {
        eprintln!("Warning: Failed to set Ctrl-C handler: {}", e);
    }

    let args = Args::parse();
    let mut skipped_log: Box<dyn Write> = if args.log_to_file {
        Box::new(fs::File::create("skipped.log").unwrap())
    } else {
        Box::new(io::stdout())
    };

    let mut failures_log: Box<dyn Write> = if args.log_to_file {
        Box::new(fs::File::create("failures.log").unwrap())
    } else {
        Box::new(io::stderr())
    };

    let wslinux = args
        .wslinux
        .or_else(|| {
            if PathBuf::from("./wslinux").exists() {
                Some(PathBuf::from("./wslinux"))
            } else if PathBuf::from("wslinux").exists() {
                Some(PathBuf::from("wslinux"))
            } else {
                None
            }
        })
        .expect("Could not find wslinux executable");

    let entries: Vec<_> = WalkDir::new(&args.directory)
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

        if let Some(start_index) = args.start_index {
            if file_num < start_index {
                continue;
            }
        }

        {
            let content = match fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let input_buffers = fs::read_to_string(path.with_extension("in.json")).ok();

            let mut current_configs = args.configs.clone();

            if content.contains("subgroup") {
                if !current_configs.is_empty() {
                    current_configs.retain(|c| c.implementation != Implementation::Wgpu);
                    if current_configs.is_empty() {
                        writeln!(
                            skipped_log,
                            "Skipping: {} (uses subgroups, no valid configs left)",
                            path.display()
                        )
                        .unwrap();
                        println!(
                            "[{}/{}] Skipped: {} (uses subgroups, no valid configs left)",
                            file_num,
                            total_files,
                            path.display()
                        );
                        continue;
                    }
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
                    continue;
                }
            };

            if !spe::filter_module(&mut module) {
                println!(
                    "[{}/{}] No compute entrypoint: {}. Running compile validations.",
                    file_num,
                    total_files,
                    path.display()
                );

                for backend in &["hlsl", "spirv", "msl"] {
                    for compiler in &["tint", "naga"] {
                        run_compile(
                            &wslinux,
                            path,
                            backend,
                            compiler,
                            &mut *failures_log,
                            &format!("(no entrypoint for {})", path.display()),
                        );
                    }
                }

                writeln!(
                    skipped_log,
                    "Skipping: {} (no compute entrypoint left after filtering)",
                    path.display()
                )
                .unwrap();
                println!(
                    "[{}/{}] Skipped: {} (no compute entrypoint)",
                    file_num,
                    total_files,
                    path.display()
                );
                continue;
            }

            let (holes, mut enumerations, original_assignment_idx) = {
                let est = spe::estimate_enumerations(&module);
                if est > 100_000 {
                    writeln!(
                        skipped_log,
                        "Skipping: {} (estimated {} bounds, > 100,000)",
                        path.display(),
                        est
                    )
                    .unwrap();
                    println!(
                        "[{}/{}] Skipped prematurely: {} (estimated {} bounds)",
                        file_num,
                        total_files,
                        path.display(),
                        est
                    );
                    continue;
                }
                match std::panic::catch_unwind(|| spe::get_enumerations(&module, None)) {
                    Ok(res) => res,
                    Err(_) => {
                        writeln!(failures_log, "Enumerate panic on: {}", path.display()).unwrap();
                        FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                        continue;
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
                continue;
            }

            println!("original_assignment_idx {:?}", original_assignment_idx);

            if enumerations.len() > 500 {
                println!(
                    "[{}/{}] Downsampling: {} ({} enumerations -> 500 randomly sampled, {} holes)",
                    file_num,
                    total_files,
                    path.display(),
                    enumerations.len(),
                    holes
                );

                let mut rng = rand::thread_rng();
                enumerations.shuffle(&mut rng);
                enumerations.truncate(500);
            } else {
                println!(
                    "[{}/{}] Processing: {} ({} enumerations, {} holes)",
                    file_num,
                    total_files,
                    path.display(),
                    enumerations.len(),
                    holes
                );
            }

            let mut failed_count = 0;
            let mut shader_has_runtime_failure = false;

            for (i, assigns) in enumerations.iter().enumerate() {
                if let Some(enum_idx) = args.enumeration {
                    if i != enum_idx {
                        continue;
                    }
                }

                let case_str = if Some(i) == original_assignment_idx {
                    format!("(case {i} (original))")
                } else {
                    format!("(case {i})")
                };

                let out_str =
                    match std::panic::catch_unwind(|| apply_assignment(&module, assigns)) {
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
                            if failed_count >= 10 {
                                println!(
                                "[{}/{}] Skipped remaining enumerations for {} (>= 10 failures)",
                                file_num, total_files,
                                path.display()
                            );
                                break;
                            }
                            continue;
                        }
                    };

                if args.print_only {
                    println!("{}", out_str);
                    continue;
                }

                let tmp_path = std::env::temp_dir().join(format!(
                    "shader_{}_{}_{}.wgsl",
                    std::process::id(),
                    file_num,
                    i
                ));

                fs::write(&tmp_path, &out_str).expect("Failed to write temporary shader file");

                let mut recond_cmd = Command::new(&wslinux);
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
                    }
                }
                if args.msl_validate {
                    let msl_tint_ok = run_compile(
                        &wslinux,
                        &tmp_path,
                        "msl",
                        "tint",
                        &mut *failures_log,
                        &format!("for {} {}", path.display(), case_str),
                    );

                    let mut msl_naga_ok = true;
                    if !content.contains("subgroup") {
                        msl_naga_ok = run_compile(
                            &wslinux,
                            &tmp_path,
                            "msl",
                            "naga",
                            &mut *failures_log,
                            &format!("for {} {}", path.display(), case_str),
                        );
                    }

                    if !msl_tint_ok || !msl_naga_ok {
                        failed_count += 1;
                        shader_has_runtime_failure = true;
                    }
                }

                let mut cmd = Command::new(&wslinux);
                cmd.arg("run");

                for config in &current_configs {
                    cmd.arg("-c").arg(config.to_string());
                }

                cmd.arg("-j");

                if let Some(j) = args.parallelism {
                    cmd.arg(j.to_string());
                } else {
                    cmd.arg("2");
                }

                if args.use_daemon {
                    cmd.arg("--use-daemon");
                }

                cmd.arg(&tmp_path);

                if let Some(input_buffers) = &input_buffers {
                    cmd.arg(input_buffers);
                }

                let output = cmd.output();
                match output {
                    Ok(out) => {
                        if !out.status.success() {
                            let stderr = String::from_utf8_lossy(&out.stderr);
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            writeln!(
                                failures_log,
                                "Failed validation for: {} {case_str}\nStdout: {}\nStderr: {}",
                                path.display(),
                                stdout,
                                stderr
                            )
                            .unwrap();
                            failed_count += 1;
                            shader_has_runtime_failure = true;
                        }
                    }
                    Err(e) => {
                        writeln!(
                            failures_log,
                            "Failed to run wslinux for: {} {case_str}\nError: {}",
                            path.display(),
                            e
                        )
                        .unwrap();
                        failed_count += 1;
                        shader_has_runtime_failure = true;
                    }
                }

                fs::remove_file(&tmp_path).ok();

                if failed_count >= 10 {
                    println!(
                        "[{}/{}] Skipped remaining enumerations for {} (>= 10 failures)",
                        file_num,
                        total_files,
                        path.display()
                    );
                    break;
                }
            }

            if shader_has_runtime_failure {
                FAILED_RUN.fetch_add(1, Ordering::SeqCst);
            } else {
                ENUMERATED_NO_ISSUE.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    print_stats();
}
