#[allow(dead_code)]
mod build_script {
    include!("../build.rs");

    pub fn selected_embed_library(
        directory: &std::path::Path,
        target_os: &str,
    ) -> Option<std::path::PathBuf> {
        embed_library_in(directory, target_os)
    }
}

use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_directory() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("pam-build-script-{}-{nonce}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    directory
}

#[test]
fn selects_only_the_windows_embed_import_library() {
    let directory = temporary_directory();
    fs::write(directory.join("php8.lib"), []).unwrap();
    fs::write(directory.join("php8embed.lib"), []).unwrap();
    fs::write(directory.join("php_embed.dll"), []).unwrap();

    assert_eq!(
        build_script::selected_embed_library(&directory, "windows"),
        Some(directory.join("php8embed.lib"))
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn windows_library_cannot_be_mistaken_for_a_unix_embed_library() {
    let directory = temporary_directory();
    fs::write(directory.join("php8embed.lib"), []).unwrap();

    assert_eq!(
        build_script::selected_embed_library(&directory, "linux"),
        None
    );
    assert_eq!(
        build_script::selected_embed_library(&directory, "macos"),
        None
    );
    fs::remove_dir_all(directory).unwrap();
}
