use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use time::{format_description, OffsetDateTime, UtcOffset};

pub fn parse_suppressing_panic(src: &str) -> Option<ast::Module> {
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let res = std::panic::catch_unwind(|| parser::parse(src));
    std::panic::set_hook(prev_hook);
    res.ok()
}

pub fn generate_inputs_if_needed(module: &ast::Module) -> Option<String> {
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

    if init_data.is_empty() {
        None
    } else {
        serde_json::to_string(&init_data).ok()
    }
}

pub fn get_logs(
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

pub fn create_out_dir(log_to_file: bool, append_dir: Option<&PathBuf>) -> (Option<PathBuf>, bool) {
    if let Some(dir) = append_dir {
        (Some(dir.clone()), true)
    } else if log_to_file {
        let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
        let now = OffsetDateTime::now_utc().to_offset(offset);
        let format =
            format_description::parse("spe-[year]-[month]-[day]-[hour]-[minute]-[second]").unwrap();
        let dir_name = now.format(&format).unwrap();
        (Some(PathBuf::from(dir_name)), false)
    } else {
        (None, false)
    }
}
