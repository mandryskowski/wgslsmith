pub mod enumerate {
    use crate::enumerator;
    use crate::options::EnumerateOptions;
    use std::fs;

    pub fn run(opt: EnumerateOptions, skip_original: bool) {
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
}

pub mod fuse {
    use crate::options::DirOptions;
    use crate::processor::ShaderProcessor;
    use crate::{stats, util, wgslsmith};
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::Ordering;
    use walkdir::WalkDir;

    pub fn run(opt: DirOptions, skip_original: bool) {
        let wgslsmith_exe = std::env::current_exe().expect("Failed to get current executable path");

        if let Some(server) = &opt.server {
            wgslsmith::do_healthcheck(&wgslsmith_exe, server, &opt.configs);
        }

        let support_map = if opt.skip_ext_filter {
            std::collections::HashMap::new()
        } else {
            wgslsmith::check_extension_support(&wgslsmith_exe, opt.server.as_ref(), &opt.configs)
        };

        let mut effective_start_index = opt.start_index;
        let log_to_file = opt.log_to_file || opt.append_dir.is_some();

        let (out_dir_opt, append) = util::create_out_dir(log_to_file, opt.append_dir.as_ref());

        if let Some(out_dir) = &out_dir_opt {
            if let Some(append_dir) = &opt.append_dir {
                let last_idx = stats::load_stats(append_dir);
                if effective_start_index.is_none() && last_idx > 0 {
                    effective_start_index = Some(last_idx);
                }
            }
            fs::create_dir_all(out_dir).unwrap();
            stats::set_out_dir(out_dir.clone());
        }

        let (mut skipped_log, mut failures_log) =
            util::get_logs(log_to_file, out_dir_opt.as_deref(), append);

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
            let content = fs::read_to_string(passed_shaders_file)
                .expect("Failed to read passed shaders file");
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
                    input_buffers = util::generate_inputs_if_needed(&module);
                }

                if let Some(reconditioned_src) =
                    wgslsmith::recondition_shader_src(&wgslsmith_exe, &content)
                {
                    if wgslsmith::test_shader_has_outputs(
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
                    use std::io::Write;
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

        let mut ignore_regexes = Vec::new();
        if let Some(ignore_file) = &opt.ignore_file {
            let content = std::fs::read_to_string(ignore_file).expect("Failed to read ignore file");
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    ignore_regexes
                        .push(regex::Regex::new(line).expect("Invalid regex in ignore file"));
                }
            }
        }

        let mut processor = ShaderProcessor {
            wgslsmith_exe: &wgslsmith_exe,
            out_dir: out_dir_opt.as_deref(),
            skipped_log: &mut *skipped_log,
            failures_log: &mut *failures_log,
            support_map: &support_map,
            max_enumerations: 5,
            skip_original,
            opt: &opt,
            ignore_regexes: &ignore_regexes,
        };

        loop {
            file_num += 1;
            stats::LAST_INDEX.store(file_num, Ordering::SeqCst);
            stats::DIRTY_STATS.store(true, Ordering::SeqCst);

            use rand::Rng;
            let count = rng.gen_range(5..=10).min(working_modules.len());

            use rand::seq::SliceRandom;
            let mut chosen_indices: Vec<usize> = (0..working_modules.len()).collect();
            chosen_indices.shuffle(&mut rng);
            chosen_indices.truncate(count);

            let mut base_module = working_modules[chosen_indices[0]].clone();
            for &idx in &chosen_indices[1..] {
                let next_module = working_modules[idx].clone();
                base_module = fuse::fuse(base_module, next_module);
            }

            let fused_inputs = util::generate_inputs_if_needed(&base_module);

            {
                let mut out_str = String::new();
                ast::writer::Writer::default()
                    .write_module(&mut out_str, &base_module)
                    .unwrap();
                println!("shader is {}", out_str);
            }

            processor.process_core(
                &base_module,
                fused_inputs,
                format!("fused_shader_{}", file_num),
                format!("fused_{}", file_num),
                file_num,
                None,
            );

            shaders_processed += 1;
            if shaders_processed % 50 == 0 {
                stats::write_stats_json();

                if let Some(server) = &opt.server {
                    wgslsmith::do_healthcheck(&wgslsmith_exe, server, &opt.configs);
                }
            }
        }
    }
}

pub mod process_dir {
    use crate::options::DirOptions;
    use crate::processor::ShaderProcessor;
    use crate::{stats, util, wgslsmith};
    use std::sync::atomic::Ordering;
    use walkdir::WalkDir;

    pub fn run(opt: DirOptions, skip_original: bool) {
        let wgslsmith_exe = std::env::current_exe().expect("Failed to get current executable path");

        if let Some(server) = &opt.server {
            wgslsmith::do_healthcheck(&wgslsmith_exe, server, &opt.configs);
        }

        let support_map = if opt.skip_ext_filter {
            std::collections::HashMap::new()
        } else {
            wgslsmith::check_extension_support(&wgslsmith_exe, opt.server.as_ref(), &opt.configs)
        };

        let mut effective_start_index = opt.start_index;
        let log_to_file = opt.log_to_file || opt.append_dir.is_some();

        let (out_dir_opt, append) = util::create_out_dir(log_to_file, opt.append_dir.as_ref());

        if let Some(out_dir) = &out_dir_opt {
            if let Some(append_dir) = &opt.append_dir {
                let last_idx = stats::load_stats(append_dir);
                if effective_start_index.is_none() && last_idx > 0 {
                    effective_start_index = Some(last_idx);
                }
            }
            std::fs::create_dir_all(out_dir).unwrap();
            stats::set_out_dir(out_dir.clone());
        }

        let (mut skipped_log, mut failures_log) =
            util::get_logs(log_to_file, out_dir_opt.as_deref(), append);

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

        let mut ignore_regexes = Vec::new();
        if let Some(ignore_file) = &opt.ignore_file {
            let content = std::fs::read_to_string(ignore_file).expect("Failed to read ignore file");
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() {
                    ignore_regexes
                        .push(regex::Regex::new(line).expect("Invalid regex in ignore file"));
                }
            }
        }

        let mut processor = ShaderProcessor {
            wgslsmith_exe: &wgslsmith_exe,
            out_dir: out_dir_opt.as_deref(),
            skipped_log: &mut *skipped_log,
            failures_log: &mut *failures_log,
            support_map: &support_map,
            max_enumerations: 100,
            skip_original,
            opt: &opt,
            ignore_regexes: &ignore_regexes,
        };

        for (file_idx, entry) in entries.into_iter().enumerate() {
            let path = entry.path();
            let file_num = file_idx + 1;

            if let Some(start_index) = effective_start_index {
                if file_num < start_index {
                    continue;
                }
            }

            stats::LAST_INDEX.store(file_num, Ordering::SeqCst);
            stats::DIRTY_STATS.store(true, Ordering::SeqCst);

            processor.process_file(path, file_num, Some(total_files));

            shaders_processed += 1;
            if shaders_processed % 50 == 0 {
                stats::write_stats_json();

                if let Some(server) = &opt.server {
                    wgslsmith::do_healthcheck(&wgslsmith_exe, server, &opt.configs);
                }
            }
        }

        stats::print_stats();
    }
}
