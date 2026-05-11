use crate::enumerator;
use crate::options::DirOptions;
use crate::stats;
use crate::util;
use crate::wgslsmith;
use ast::EnableExtension;
use harness_types::ConfigId;
use rand::seq::SliceRandom;
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::process;
use std::sync::atomic::Ordering;

pub struct ShaderProcessor<'a> {
    pub wgslsmith_exe: &'a Path,
    pub out_dir: Option<&'a Path>,
    pub skipped_log: &'a mut dyn Write,
    pub failures_log: &'a mut dyn Write,
    pub support_map: &'a HashMap<EnableExtension, HashSet<ConfigId>>,
    pub max_enumerations: usize,
    pub skip_original: bool,
    pub opt: &'a DirOptions,
    pub ignore_regexes: &'a [regex::Regex],
}

impl<'a> ShaderProcessor<'a> {
    pub fn process_file(&mut self, path: &Path, file_num: usize, total_files: Option<usize>) {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut input_buffers = std::fs::read_to_string(path.with_extension("in.json")).ok();

        let module = match std::panic::catch_unwind(|| parser::parse(&content)) {
            Ok(m) => m,
            Err(_) => {
                writeln!(
                    self.failures_log,
                    "[{}] [{}] Parse panic on: {}",
                    stats::current_timestamp(),
                    file_num,
                    path.display()
                )
                .unwrap();
                stats::STAT_FAILED_PARSE.fetch_add(1, Ordering::SeqCst);
                return;
            }
        };

        if input_buffers.is_none() {
            input_buffers = util::generate_inputs_if_needed(&module);
        }

        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        self.process_core(
            &module,
            input_buffers,
            path.display().to_string(),
            stem,
            file_num,
            total_files,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn process_core(
        &mut self,
        module: &ast::Module,
        input_buffers: Option<String>,
        path_display: String,
        stem: String,
        file_num: usize,
        total_files: Option<usize>,
        fused_paths: Option<Vec<std::path::PathBuf>>,
    ) {
        let progress_prefix = if let Some(total) = total_files {
            format!("[{}/{}] ", file_num, total)
        } else {
            "".to_string()
        };

        let mut current_configs = self.opt.configs.clone();

        for ext in &module.enables {
            if let Some(supported) = self.support_map.get(ext) {
                let original_len = current_configs.len();
                current_configs.retain(|c| supported.contains(c));

                if current_configs.len() < original_len {
                    writeln!(
                        self.skipped_log,
                        "[{}] [{}] Filtered configs for: {} (uses {})",
                        stats::current_timestamp(),
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
                self.skipped_log,
                "[{}] [{}] Skipping: {} (uses extensions not supported by any config)",
                stats::current_timestamp(),
                file_num,
                path_display
            )
            .unwrap();
            println!(
                "[{}] {}Skipped: {} (uses extensions not supported by any config)",
                stats::current_timestamp(),
                progress_prefix,
                path_display
            );
            return;
        }

        let (holes, mut enumerations, original_assignment_idx) = {
            let est = enumerator::estimate_enumerations(module);
            let search_limit = std::cmp::min(2000, self.max_enumerations) - 1;
            let limit = if est > 100_000 {
                writeln!(
                    self.skipped_log,
                    "[{}] [{}] Warning: {} (estimated {} bounds, > 100,000). Limiting search to {search_limit} variants.",
                    stats::current_timestamp(),
                    file_num,
                    path_display,
                    est
                )
                .unwrap();
                println!(
                    "[{}] {}Large enumeration space: {} (estimated {} bounds). Limiting search to {search_limit} variants.",
                    stats::current_timestamp(),
                    progress_prefix,
                    path_display,
                    est
                );
                Some(search_limit)
            } else {
                None
            };
            match std::panic::catch_unwind(|| enumerator::get_enumerations(module, limit)) {
                Ok(res) => res,
                Err(_) => {
                    writeln!(
                        self.failures_log,
                        "[{}] [{}] Enumerate panic on: {}",
                        stats::current_timestamp(),
                        file_num,
                        path_display
                    )
                    .unwrap();
                    stats::STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                    return;
                }
            }
        };

        if original_assignment_idx.is_none() {
            writeln!(
                self.failures_log,
                "[{}] [{}] No original assignment found for: {}",
                stats::current_timestamp(),
                file_num,
                path_display
            )
            .unwrap();
            stats::STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
            return;
        }

        let original_assignment = original_assignment_idx.map(|idx| enumerations[idx].clone());

        if self.skip_original && enumerations.len() <= 1 {
            writeln!(
                self.skipped_log,
                "[{}] [{}] Skipping: {} (only original enumeration exists)",
                stats::current_timestamp(),
                file_num,
                path_display
            )
            .unwrap();
            println!(
                "[{}] {}Skipped: {} (only original enumeration exists)",
                stats::current_timestamp(),
                progress_prefix,
                path_display
            );
            stats::STAT_SKIPPED_ONLY_ORIGINAL.fetch_add(1, Ordering::SeqCst);
            return;
        }

        // Rearrange so original is always at the beginning
        if let Some(orig) = &original_assignment {
            enumerations.retain(|e| e != orig);
            enumerations.insert(0, orig.clone());
        }

        if enumerations.len() > self.max_enumerations {
            println!(
                "[{}] {}Downsampling: {} ({} enumerations -> {} randomly sampled, {} holes)",
                stats::current_timestamp(),
                progress_prefix,
                path_display,
                enumerations.len(),
                self.max_enumerations,
                holes
            );
            stats::STAT_SUBSAMPLED.fetch_add(1, Ordering::SeqCst);

            let mut rng = rand::thread_rng();
            if original_assignment.is_some() {
                let first = enumerations.remove(0);
                enumerations.shuffle(&mut rng);
                enumerations.truncate(self.max_enumerations.saturating_sub(1));
                enumerations.insert(0, first);
            } else {
                enumerations.shuffle(&mut rng);
                enumerations.truncate(self.max_enumerations);
            }
        } else {
            println!(
                "[{}] {}Processing: {} ({} enumerations, {} holes)",
                stats::current_timestamp(),
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

            let out_str = match std::panic::catch_unwind(|| {
                enumerator::apply_assignment(module, assigns)
            }) {
                Ok(s) => s,
                Err(_) => {
                    writeln!(
                        self.failures_log,
                        "[{}] [{}] Apply assignment panic on: {} {case_str}",
                        stats::current_timestamp(),
                        file_num,
                        path_display
                    )
                    .unwrap();
                    failed_count += 1;
                    if is_original {
                        stats::STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                        println!(
                                "[{}] {}Skipped variants for {} (original panicked on apply_assignment)",
                                stats::current_timestamp(),
                                progress_prefix,
                                path_display
                            );
                        break;
                    }
                    if failed_count >= 10 {
                        println!(
                            "[{}] {}Skipped remaining enumerations for {} (>= 10 failures)",
                            stats::current_timestamp(),
                            progress_prefix,
                            path_display
                        );
                        break;
                    }
                    continue;
                }
            };

            if self.opt.print {
                println!("// === {} {} ===\n{}", path_display, case_str, out_str);
            }

            let mut current_src = out_str.clone();

            if let Some(reconditioned) =
                wgslsmith::recondition_shader_src(self.wgslsmith_exe, &current_src)
            {
                current_src = format!("// {path_display}_{i}\n{reconditioned}");
            } else {
                writeln!(
                    self.failures_log,
                    "[{}] [{}] Recondition failed for: {} {}",
                    stats::current_timestamp(),
                    file_num,
                    path_display,
                    case_str
                )
                .unwrap();

                if let Some(out_dir) = self.out_dir {
                    let recond_out = out_dir.join("recondition-out");
                    let failure_out_dir = recond_out.join(format!("{}_{}-{}", stem, file_num, i));
                    std::fs::create_dir_all(&failure_out_dir).unwrap();

                    std::fs::write(failure_out_dir.join("shader.wgsl"), &current_src).unwrap();

                    if let Some(fused) = &fused_paths {
                        let paths_str = fused
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join("\n");
                        std::fs::write(failure_out_dir.join("fused_from.txt"), paths_str).unwrap();
                    }
                }

                if is_original {
                    stats::STAT_FAILED_RECONDITION.fetch_add(1, Ordering::SeqCst);
                    println!(
                        "[{}] {}Skipped variants for {} (original failed recondition)",
                        stats::current_timestamp(),
                        progress_prefix,
                        path_display
                    );
                    break;
                }

                continue;
            }

            if self.opt.msl_validate {
                let msl_tint_ok = wgslsmith::run_compile(
                    self.wgslsmith_exe,
                    &current_src,
                    "msl",
                    "tint",
                    self.failures_log,
                    &format!("[{}] for {} {}", file_num, path_display, case_str),
                );

                let mut msl_naga_ok = true;
                if !module.enables.contains(&ast::EnableExtension::Subgroups) {
                    msl_naga_ok = wgslsmith::run_compile(
                        self.wgslsmith_exe,
                        &current_src,
                        "msl",
                        "naga",
                        self.failures_log,
                        &format!("[{}] for {} {}", file_num, path_display, case_str),
                    );
                }

                if !msl_tint_ok || !msl_naga_ok {
                    failed_count += 1;

                    if is_original {
                        stats::STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                        println!(
                            "[{}] {}Skipped variants for {} (original failed msl_validate)",
                            stats::current_timestamp(),
                            progress_prefix,
                            path_display
                        );
                        break;
                    }
                }
            }

            let mut cmd = process::Command::new(self.wgslsmith_exe);

            if let Some(server) = &self.opt.server {
                cmd.arg("remote").arg(server);
            }

            cmd.arg("run");

            for config in &current_configs {
                cmd.arg("-c").arg(config.to_string());
            }

            cmd.arg("-j")
                .arg(self.opt.parallelism.unwrap_or(2).to_string());

            if self.opt.use_daemon {
                cmd.arg("--use-daemon");
                if let Some(daemon_port) = self.opt.daemon_port {
                    cmd.arg("--daemon-port").arg(daemon_port.to_string());
                }
            }

            cmd.arg("--print-consensus").arg("-");

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
                        let mut ignored = false;
                        for r in self.ignore_regexes {
                            if r.is_match(&combined_output) {
                                ignored = true;
                                break;
                            }
                        }

                        if ignored {
                            writeln!(
                                self.failures_log,
                                "[{}] [{}] Ignored validation failure for: {} {}",
                                stats::current_timestamp(),
                                file_num,
                                path_display,
                                case_str
                            )
                            .unwrap();
                        } else {
                            writeln!(
                                self.failures_log,
                                "[{}] [{}] Failed validation for: {} {case_str}\nStdout: {}\nStderr: {}",
                                stats::current_timestamp(),
                                file_num,
                                path_display,
                                stdout_str,
                                stderr_str
                            )
                            .unwrap();
                        }

                        failed_count += 1;

                        if !ignored {
                            let kind = if out.status.code() == Some(1) {
                                "mismatch"
                            } else {
                                "crash"
                            };

                            failures_to_save.push((
                                i,
                                kind.to_string(),
                                consensus_json.into_bytes(),
                                combined_bytes,
                                out_str.clone(),
                                current_src.clone(),
                                is_original,
                            ));
                        }

                        if is_original {
                            stats::STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                            println!(
                                "[{}] {}Skipped variants for {} (original failed validation)",
                                stats::current_timestamp(),
                                progress_prefix,
                                path_display
                            );
                            break;
                        }
                    } else {
                        if is_original {
                            stats::STAT_RUN_SUCCESS.fetch_add(1, Ordering::SeqCst);
                        }
                        has_success = true;
                    }
                }
                Err(e) => {
                    writeln!(
                        self.failures_log,
                        "[{}] [{}] Failed to run wgslsmith for: {} {case_str}\nError: {}",
                        stats::current_timestamp(),
                        file_num,
                        path_display,
                        e
                    )
                    .unwrap();
                    failed_count += 1;

                    failures_to_save.push((
                        i,
                        "crash".to_string(),
                        Vec::new(),
                        format!("Error: {}", e).into_bytes(),
                        out_str.clone(),
                        current_src.clone(),
                        is_original,
                    ));

                    if is_original {
                        stats::STAT_FAILED_RUN.fetch_add(1, Ordering::SeqCst);
                        println!(
                            "[{}] {}Skipped variants for {} (original failed execution)",
                            stats::current_timestamp(),
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
                    stats::current_timestamp(),
                    progress_prefix,
                    path_display
                );
                break;
            }
        }

        if !failures_to_save.is_empty() {
            if let Some(out_dir) = self.out_dir {
                for (i, kind, consensus, combined, src, recond_src, is_original) in failures_to_save
                {
                    if !is_original && !has_success {
                        continue;
                    }

                    let base_out = if is_original {
                        out_dir.join("original-out")
                    } else {
                        out_dir.join("out")
                    };
                    let failure_out_dir =
                        base_out.join(format!("{}_{}-{}-{kind}", stem, file_num, i));
                    std::fs::create_dir_all(&failure_out_dir).unwrap();

                    std::fs::write(failure_out_dir.join("shader.wgsl"), src).unwrap();
                    std::fs::write(failure_out_dir.join("reconditioned.wgsl"), recond_src).unwrap();

                    if let Some(in_bufs) = &input_buffers {
                        std::fs::write(failure_out_dir.join("inputs.json"), in_bufs).unwrap();
                    }

                    let configs_str = self
                        .opt
                        .configs
                        .iter()
                        .map(|c| format!("\"{}\"", c))
                        .collect::<Vec<_>>()
                        .join(", ");

                    let name = self.opt.server.clone().unwrap_or_else(|| {
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

                    if let Some(fused) = &fused_paths {
                        let paths_str = fused
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join("\n");
                        std::fs::write(failure_out_dir.join("fused_from.txt"), paths_str).unwrap();
                    }
                }
            }
        }
    }
}
