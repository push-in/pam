use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMPORARY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn write_file_transactionally(
    destination: &Path,
    contents: &[u8],
    label: &str,
) -> Result<(), String> {
    let temporary = temporary_sibling(destination, "write");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| format!("cannot allocate {label}: {error}"))?;
    let write_result = output.write_all(contents).and_then(|()| output.sync_all());
    drop(output);
    let result = write_result
        .map_err(|error| format!("cannot write {label}: {error}"))
        .and_then(|()| publish_file_transactionally(&temporary, destination, label));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn publish_file_transactionally(
    temporary: &Path,
    destination: &Path,
    label: &str,
) -> Result<(), String> {
    let backup = destination.with_extension("previous");
    let destination_exists = regular_file_state(destination, &format!("existing {label}"))?;
    let backup_exists = regular_file_state(&backup, &format!("{label} backup"))?;
    let had_destination = match (destination_exists, backup_exists) {
        (true, false) => {
            fs::rename(destination, &backup)
                .map_err(|error| format!("cannot preserve previous {label}: {error}"))?;
            true
        }
        (false, true) => true,
        (false, false) => false,
        (true, true) => {
            return Err(format!(
                "{label} activation has both active and backup files"
            ));
        }
    };
    if let Err(error) = fs::rename(temporary, destination) {
        if had_destination {
            fs::rename(&backup, destination).map_err(|rollback| {
                format!("cannot publish {label}: {error}; rollback failed: {rollback}")
            })?;
        }
        return Err(format!("cannot publish {label}: {error}"));
    }
    if had_destination {
        fs::remove_file(&backup)
            .map_err(|error| format!("cannot remove previous {label}: {error}"))?;
    }
    sync_parent(destination)
}

pub(crate) fn temporary_sibling(destination: &Path, purpose: &str) -> PathBuf {
    let sequence = TEMPORARY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    destination.with_extension(format!("{purpose}-{}-{sequence}", std::process::id()))
}

fn regular_file_state(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => Ok(true),
        Ok(_) => Err(format!(
            "refusing unexpected non-file {label}: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("cannot inspect {label}: {error}")),
    }
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "transaction destination has no parent directory".to_owned())?;
    fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("cannot persist transaction directory: {error}"))
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "pam-desktop-transaction-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).expect("fixture directory");
        directory
    }

    #[test]
    fn replaces_a_file_after_closing_its_temporary_handle() {
        let directory = fixture("replace");
        let destination = directory.join("host-state.json");
        fs::write(&destination, b"previous").expect("previous state");

        write_file_transactionally(&destination, b"verified", "Desktop state").expect("publish");

        assert_eq!(fs::read(&destination).expect("active state"), b"verified");
        assert!(!destination.with_extension("previous").exists());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn restores_the_previous_file_when_activation_fails() {
        let directory = fixture("rollback");
        let destination = directory.join("pam-desktop");
        let missing = destination.with_extension("missing");
        fs::write(&destination, b"previous").expect("previous host");

        let error = publish_file_transactionally(&missing, &destination, "Desktop host")
            .expect_err("missing replacement must fail");

        assert!(error.contains("cannot publish Desktop host"));
        assert_eq!(fs::read(&destination).expect("restored host"), b"previous");
        assert!(!destination.with_extension("previous").exists());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn recovers_a_backup_left_by_an_interrupted_activation() {
        let directory = fixture("interrupted");
        let destination = directory.join("pam-desktop");
        let backup = destination.with_extension("previous");
        let missing = destination.with_extension("missing");
        fs::write(&backup, b"previous").expect("interrupted backup");

        publish_file_transactionally(&missing, &destination, "Desktop host")
            .expect_err("missing replacement must fail");

        assert_eq!(fs::read(&destination).expect("restored host"), b"previous");
        assert!(!backup.exists());
        fs::remove_dir_all(directory).expect("cleanup");
    }

    #[test]
    fn allocates_distinct_same_directory_temporary_paths() {
        let destination = Path::new("pam-desktop");
        let first = temporary_sibling(destination, "download");
        let second = temporary_sibling(destination, "download");
        assert_ne!(first, second);
        assert_eq!(first.parent(), destination.parent());
    }
}
