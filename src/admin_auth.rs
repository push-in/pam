use std::fs::{self, OpenOptions};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[cfg(windows)]
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;

use sha2::{Digest, Sha256};

pub const ADMIN_TOKEN_ENV: &str = "PAM_ADMIN_TOKEN";
pub const ADMIN_TOKEN_FILE_ENV: &str = "PAM_ADMIN_TOKEN_FILE";
const MIN_TOKEN_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 256;

pub struct AdminCredential {
    token: String,
}

impl AdminCredential {
    pub fn as_str(&self) -> &str {
        &self.token
    }

    pub fn digest(&self) -> [u8; 32] {
        Sha256::digest(self.token.as_bytes()).into()
    }
}

impl Drop for AdminCredential {
    fn drop(&mut self) {
        // SAFETY: zero bytes preserve UTF-8 validity and the value is not
        // observed again after Drop begins.
        unsafe {
            self.token.as_mut_str().as_bytes_mut().fill(0);
        }
    }
}

pub fn load() -> Result<Option<AdminCredential>, String> {
    let direct = environment_value(ADMIN_TOKEN_ENV)?;
    let file = environment_value(ADMIN_TOKEN_FILE_ENV)?;
    match (direct, file) {
        (Some(_), Some(_)) => Err(format!(
            "set only one of {ADMIN_TOKEN_ENV} or {ADMIN_TOKEN_FILE_ENV}"
        )),
        (Some(token), None) => validate(token).map(Some),
        (None, Some(path)) => read_file(Path::new(&path)).map(Some),
        (None, None) => Ok(None),
    }
}

fn environment_value(name: &str) -> Result<Option<String>, String> {
    match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn read_file(path: &Path) -> Result<AdminCredential, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect {ADMIN_TOKEN_FILE_ENV} {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "{ADMIN_TOKEN_FILE_ENV} must reference a regular non-symlink file"
        ));
    }
    if metadata.len() > (MAX_TOKEN_BYTES + 2) as u64 {
        return Err(format!(
            "{ADMIN_TOKEN_FILE_ENV} exceeds the {}-byte limit",
            MAX_TOKEN_BYTES + 2
        ));
    }
    let file = open_without_following_links(path).map_err(|error| {
        format!(
            "cannot open {ADMIN_TOKEN_FILE_ENV} {}: {error}",
            path.display()
        )
    })?;
    let opened = file.metadata().map_err(|error| {
        format!(
            "cannot inspect opened {ADMIN_TOKEN_FILE_ENV} {}: {error}",
            path.display()
        )
    })?;
    if !opened.is_file() || file_identity(&opened) != file_identity(&metadata) {
        return Err(format!(
            "{ADMIN_TOKEN_FILE_ENV} changed while it was being opened"
        ));
    }
    let mut bytes = Vec::new();
    file.take((MAX_TOKEN_BYTES + 3) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            format!(
                "cannot read {ADMIN_TOKEN_FILE_ENV} {}: {error}",
                path.display()
            )
        })?;
    if bytes.len() > MAX_TOKEN_BYTES + 2 {
        return Err(format!(
            "{ADMIN_TOKEN_FILE_ENV} exceeds the {}-byte limit",
            MAX_TOKEN_BYTES + 2
        ));
    }
    if bytes.ends_with(b"\r\n") {
        bytes.truncate(bytes.len() - 2);
    } else if bytes.ends_with(b"\n") {
        bytes.pop();
    }
    let token = String::from_utf8(bytes)
        .map_err(|_| format!("{ADMIN_TOKEN_FILE_ENV} must contain valid ASCII"))?;
    validate(token)
}

#[cfg(unix)]
fn open_without_following_links(path: &Path) -> std::io::Result<std::fs::File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(windows)]
fn open_without_following_links(path: &Path) -> std::io::Result<std::fs::File> {
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(unix)]
fn file_identity(metadata: &fs::Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

#[cfg(windows)]
fn file_identity(metadata: &fs::Metadata) -> (Option<u32>, Option<u64>) {
    (metadata.volume_serial_number(), metadata.file_index())
}

pub(crate) fn validate(token: String) -> Result<AdminCredential, String> {
    if !(MIN_TOKEN_BYTES..=MAX_TOKEN_BYTES).contains(&token.len())
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(format!(
            "admin token must contain {MIN_TOKEN_BYTES} to {MAX_TOKEN_BYTES} non-whitespace ASCII characters"
        ));
    }
    Ok(AdminCredential { token })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "pam-admin-auth-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).unwrap();
        directory
    }

    #[test]
    fn validates_header_safe_tokens() {
        let token = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            validate(token.to_owned()).unwrap().digest(),
            <[u8; 32]>::from(Sha256::digest(token.as_bytes()))
        );
        for invalid in [
            "short".to_owned(),
            "a".repeat(257),
            format!("{} ", "a".repeat(31)),
            format!("{}é", "a".repeat(31)),
        ] {
            assert!(validate(invalid).is_err());
        }
    }

    #[test]
    fn reads_bounded_regular_token_files_with_one_trailing_newline() {
        let directory = temporary_directory("file");
        let path = directory.join("token");
        fs::write(&path, b"0123456789abcdef0123456789abcdef\n").unwrap();
        assert_eq!(
            read_file(&path).unwrap().as_str(),
            "0123456789abcdef0123456789abcdef"
        );

        fs::write(&path, vec![b'a'; MAX_TOKEN_BYTES + 3]).unwrap();
        assert!(read_file(&path).err().unwrap().contains("limit"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_token_files() {
        let directory = temporary_directory("symlink");
        let target = directory.join("target");
        let link = directory.join("token");
        fs::write(&target, b"0123456789abcdef0123456789abcdef").unwrap();
        symlink(&target, &link).unwrap();
        assert!(read_file(&link).err().unwrap().contains("non-symlink"));
        fs::remove_dir_all(directory).unwrap();
    }
}
