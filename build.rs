//! Build script: derive the version string shown by `abrightd --version`.
//!
//! * at an exact git tag  -> the tag (with a leading `v` stripped, e.g. `0.2.0`)
//! * anywhere else       -> `dev` (i.e. the main branch)
//! * no git / tarball    -> `dev`
//!
//! Releases just need to be tagged (`git tag v0.2.0`); nothing else to edit.

use std::path::Path;
use std::process::Command;

fn main() {
    // Rebuild when the checked-out revision or the tags change.
    for path in [".git/HEAD", ".git/refs/tags"] {
        if Path::new(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    println!(
        "cargo:rustc-env=ABRIGHTD_VERSION={}",
        git_exact_tag().unwrap_or_else(|| "dev".to_string())
    );
}

fn git_exact_tag() -> Option<String> {
    let output = Command::new("git")
        .args(["describe", "--tags", "--exact-match"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let tag = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if tag.is_empty() {
        None
    } else {
        Some(tag.trim_start_matches('v').to_string())
    }
}
