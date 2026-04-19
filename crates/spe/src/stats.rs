use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::OnceLock;
use time::{format_description, OffsetDateTime, UtcOffset};

pub static STAT_FAILED_PARSE: AtomicUsize = AtomicUsize::new(0);
pub static STAT_FAILED_RECONDITION: AtomicUsize = AtomicUsize::new(0);
pub static STAT_FAILED_RUN: AtomicUsize = AtomicUsize::new(0);
pub static STAT_RUN_SUCCESS: AtomicUsize = AtomicUsize::new(0);
pub static STAT_SKIPPED_ONLY_ORIGINAL: AtomicUsize = AtomicUsize::new(0);
pub static STAT_SUBSAMPLED: AtomicUsize = AtomicUsize::new(0);
pub static LAST_INDEX: AtomicUsize = AtomicUsize::new(0);
pub static DIRTY_STATS: AtomicBool = AtomicBool::new(false);

static OUT_DIR: OnceLock<PathBuf> = OnceLock::new();
static UTC_OFFSET: OnceLock<UtcOffset> = OnceLock::new();

pub fn init() {
    UTC_OFFSET.get_or_init(|| UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC));
}

pub fn set_out_dir(path: PathBuf) {
    let _ = OUT_DIR.set(path);
}

pub fn current_timestamp() -> String {
    let offset = *UTC_OFFSET.get().unwrap_or(&UtcOffset::UTC);
    let now = OffsetDateTime::now_utc().to_offset(offset);
    let format = format_description::parse("[hour]-[minute]-[second]").unwrap();
    now.format(&format).unwrap_or_default()
}

pub fn write_stats_json() {
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

pub fn print_stats() {
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

pub fn load_stats(out_dir: &Path) -> usize {
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
