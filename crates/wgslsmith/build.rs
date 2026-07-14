use std::env;
use std::fs;
use std::process::Command;

fn get_git_info(dir: &str, commit: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["log", "-1", "--format=%H (%cd)", "--date=short", commit])
        .current_dir(dir)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

fn is_dir_dirty(dir: &str, path: &str) -> bool {
    let output = Command::new("git")
        .args(["status", "--porcelain", path])
        .current_dir(dir)
        .output();

    if let Ok(output) = output {
        output.status.success() && !output.stdout.is_empty()
    } else {
        false
    }
}

fn main() {
    println!("cargo:rerun-if-env-changed=DAWN_BUILD_DIR");

    let mut wgslsmith_hash = get_git_info("../..", "HEAD").unwrap_or_else(|| "unknown".to_string());
    if is_dir_dirty("../..", "crates") {
        wgslsmith_hash.push_str(" (dirty)");
    }

    let dawn_hash = if let Ok(dawn_build_dir) = env::var("DAWN_BUILD_DIR") {
        let commit_path = std::path::Path::new(&dawn_build_dir).join("COMMIT");
        if let Ok(commit_content) = fs::read_to_string(commit_path) {
            let commit = commit_content.trim();
            if let Some(info) = get_git_info("../../external/dawn", commit) {
                format!("{} (pre-built)", info)
            } else {
                format!("{} (pre-built)", commit)
            }
        } else {
            "unknown (pre-built)".to_string()
        }
    } else {
        get_git_info("../../external/dawn", "HEAD").unwrap_or_else(|| "unknown".to_string())
    };

    let wgpu_hash =
        get_git_info("../../external/wgpu", "HEAD").unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=WGSLSMITH_GIT_HASH={}", wgslsmith_hash);
    println!("cargo:rustc-env=DAWN_GIT_HASH={}", dawn_hash);
    println!("cargo:rustc-env=WGPU_GIT_HASH={}", wgpu_hash);
}
