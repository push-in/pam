use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;

const CAPABILITY_FILESYSTEM_READ: u8 = 1;
const CAPABILITY_FILESYSTEM_WRITE: u8 = 2;
const CAPABILITY_NETWORK: u8 = 3;
const CAPABILITY_PROCESS: u8 = 4;
const CAPABILITY_ENVIRONMENT: u8 = 5;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    schema_version: u8,
    capabilities: Vec<Capability>,
}

#[derive(Deserialize)]
struct Capability {
    kind: u8,
    #[serde(default)]
    resources: Vec<String>,
}

pub struct Policy {
    read_paths: Vec<PathBuf>,
    write_paths: Vec<PathBuf>,
    environment: BTreeSet<String>,
    allow_network: bool,
    allow_processes: bool,
}

impl Policy {
    pub fn load(manifest_path: &Path) -> Result<Self, String> {
        let contents = std::fs::read(manifest_path)
            .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
        let manifest: Manifest = serde_json::from_slice(&contents)
            .map_err(|error| format!("invalid capability manifest: {error}"))?;
        if manifest.schema_version != 1 {
            return Err(format!(
                "unsupported capability manifest schema {}",
                manifest.schema_version
            ));
        }
        let root = manifest_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .map_err(|error| format!("cannot resolve sandbox root: {error}"))?;
        let mut resources = BTreeMap::<u8, Vec<String>>::new();
        for capability in manifest.capabilities {
            if !(CAPABILITY_FILESYSTEM_READ..=CAPABILITY_ENVIRONMENT).contains(&capability.kind) {
                return Err(format!(
                    "unknown capability kind {}; expected an integer from 1 to 5",
                    capability.kind
                ));
            }
            resources
                .entry(capability.kind)
                .or_default()
                .extend(capability.resources);
        }

        let read_paths = resolve_paths(
            &root,
            resources
                .remove(&CAPABILITY_FILESYSTEM_READ)
                .unwrap_or_default(),
        )?;
        let write_paths = resolve_paths(
            &root,
            resources
                .remove(&CAPABILITY_FILESYSTEM_WRITE)
                .unwrap_or_default(),
        )?;
        let allow_network = wildcard_or_absent(
            "network",
            resources.remove(&CAPABILITY_NETWORK).unwrap_or_default(),
        )?;
        let allow_processes = wildcard_or_absent(
            "process",
            resources.remove(&CAPABILITY_PROCESS).unwrap_or_default(),
        )?;
        let environment = resources
            .remove(&CAPABILITY_ENVIRONMENT)
            .unwrap_or_default()
            .into_iter()
            .map(|name| {
                if name.is_empty()
                    || !name
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                {
                    return Err(format!("invalid environment capability {name:?}"));
                }
                Ok(name)
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        Ok(Self {
            read_paths,
            write_paths,
            environment,
            allow_network,
            allow_processes,
        })
    }

    pub fn apply(&self) -> Result<(), String> {
        self.filter_environment();
        let mut read_paths = self.read_paths.clone();
        for support_path in [
            "/etc/php",
            "/etc/ssl/certs",
            "/usr/lib/php",
            "/usr/share/zoneinfo",
            "/dev/null",
            "/dev/urandom",
        ] {
            let path = Path::new(support_path);
            if path.exists() {
                read_paths.push(path.canonicalize().map_err(|error| {
                    format!("cannot resolve runtime support path {support_path}: {error}")
                })?);
            }
        }
        read_paths.sort();
        read_paths.dedup();
        apply_filesystem(&read_paths, &self.write_paths)?;
        apply_syscall_filter(self.allow_network, self.allow_processes)
    }

    fn filter_environment(&self) {
        let allowed = &self.environment;
        for (name, _) in std::env::vars_os() {
            if name == "PAM_INI_ENTRIES"
                || name == "PAM_SANDBOX_ACTIVE"
                || name.to_str().is_some_and(|name| allowed.contains(name))
            {
                continue;
            }
            // SAFETY: sandbox application happens before PHP, Tokio, or any
            // worker thread starts, so the process environment is single-owner.
            unsafe { std::env::remove_var(name) };
        }
        // SAFETY: same single-threaded initialization boundary as above.
        unsafe { std::env::set_var("PAM_SANDBOX_ACTIVE", "1") };
    }
}

fn resolve_paths(root: &Path, resources: Vec<String>) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    for resource in resources {
        let path = Path::new(&resource);
        if path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::Prefix(_) | Component::RootDir
            )
        }) {
            return Err(format!(
                "sandbox filesystem resource must be relative: {resource:?}"
            ));
        }
        let resolved = root.join(path).canonicalize().map_err(|error| {
            format!("cannot resolve sandbox filesystem resource {resource:?}: {error}")
        })?;
        if !resolved.starts_with(root) {
            return Err(format!(
                "sandbox filesystem resource escapes its root: {resource:?}"
            ));
        }
        paths.push(resolved);
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn wildcard_or_absent(name: &str, resources: Vec<String>) -> Result<bool, String> {
    match resources.as_slice() {
        [] => Ok(false),
        [resource] if resource == "*" => Ok(true),
        _ => Err(format!(
            "{name} capabilities currently require exactly [\"*\"]; selective {name} access needs the PAM broker and is denied closed"
        )),
    }
}

#[cfg(target_os = "linux")]
fn apply_filesystem(read_paths: &[PathBuf], write_paths: &[PathBuf]) -> Result<(), String> {
    use std::ffi::{c_int, c_long, c_void};

    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;
    const LANDLOCK_RULE_PATH_BENEATH: u32 = 1;
    const ACCESS_EXECUTE: u64 = 1 << 0;
    const ACCESS_WRITE_FILE: u64 = 1 << 1;
    const ACCESS_READ_FILE: u64 = 1 << 2;
    const ACCESS_READ_DIR: u64 = 1 << 3;
    const ACCESS_REMOVE_DIR: u64 = 1 << 4;
    const ACCESS_REMOVE_FILE: u64 = 1 << 5;
    const ACCESS_MAKE_CHAR: u64 = 1 << 6;
    const ACCESS_MAKE_DIR: u64 = 1 << 7;
    const ACCESS_MAKE_REG: u64 = 1 << 8;
    const ACCESS_MAKE_SOCK: u64 = 1 << 9;
    const ACCESS_MAKE_FIFO: u64 = 1 << 10;
    const ACCESS_MAKE_BLOCK: u64 = 1 << 11;
    const ACCESS_MAKE_SYM: u64 = 1 << 12;
    const ACCESS_REFER: u64 = 1 << 13;
    const ACCESS_TRUNCATE: u64 = 1 << 14;
    const PR_SET_NO_NEW_PRIVS: c_int = 38;

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    const SYS_LANDLOCK_CREATE_RULESET: c_long = 444;
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    const SYS_LANDLOCK_ADD_RULE: c_long = 445;
    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    const SYS_LANDLOCK_RESTRICT_SELF: c_long = 446;

    #[repr(C)]
    struct RulesetAttr {
        handled_access_fs: u64,
    }

    #[repr(C)]
    struct PathBeneathAttr {
        allowed_access: u64,
        parent_fd: i32,
        reserved: u32,
    }

    unsafe extern "C" {
        fn syscall(number: c_long, ...) -> c_long;
        fn prctl(option: c_int, ...) -> c_int;
        fn close(fd: c_int) -> c_int;
    }

    // SAFETY: a null attribute with VERSION queries the kernel ABI without
    // mutating process state.
    let abi = unsafe {
        syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<c_void>(),
            0_usize,
            LANDLOCK_CREATE_RULESET_VERSION,
        )
    };
    if abi < 1 {
        return Err(
            "Linux Landlock is unavailable; refusing to weaken filesystem sandbox".to_owned(),
        );
    }
    let mut handled = ACCESS_EXECUTE
        | ACCESS_WRITE_FILE
        | ACCESS_READ_FILE
        | ACCESS_READ_DIR
        | ACCESS_REMOVE_DIR
        | ACCESS_REMOVE_FILE
        | ACCESS_MAKE_CHAR
        | ACCESS_MAKE_DIR
        | ACCESS_MAKE_REG
        | ACCESS_MAKE_SOCK
        | ACCESS_MAKE_FIFO
        | ACCESS_MAKE_BLOCK
        | ACCESS_MAKE_SYM;
    if abi >= 2 {
        handled |= ACCESS_REFER;
    }
    if abi >= 3 {
        handled |= ACCESS_TRUNCATE;
    }
    let ruleset_attr = RulesetAttr {
        handled_access_fs: handled,
    };
    // SAFETY: the attribute points to a fully initialized C-compatible value.
    let ruleset_fd = unsafe {
        syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            &ruleset_attr,
            std::mem::size_of::<RulesetAttr>(),
            0_u32,
        )
    } as c_int;
    if ruleset_fd < 0 {
        return Err(format!(
            "cannot create Landlock ruleset: {}",
            std::io::Error::last_os_error()
        ));
    }

    let read_access = ACCESS_EXECUTE | ACCESS_READ_FILE | ACCESS_READ_DIR;
    let write_access = handled;
    let result = (|| {
        for (path, access) in read_paths
            .iter()
            .map(|path| (path, read_access))
            .chain(write_paths.iter().map(|path| (path, write_access)))
        {
            let file = File::open(path).map_err(|error| {
                format!("cannot open sandbox resource {}: {error}", path.display())
            })?;
            let metadata = file
                .metadata()
                .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
            let allowed_access = if metadata.is_dir() {
                access
            } else {
                access & (ACCESS_EXECUTE | ACCESS_WRITE_FILE | ACCESS_READ_FILE | ACCESS_TRUNCATE)
            };
            let attr = PathBeneathAttr {
                allowed_access,
                parent_fd: file.as_raw_fd(),
                reserved: 0,
            };
            // SAFETY: ruleset and parent FDs remain open for this synchronous call.
            let status = unsafe {
                syscall(
                    SYS_LANDLOCK_ADD_RULE,
                    ruleset_fd,
                    LANDLOCK_RULE_PATH_BENEATH,
                    &attr,
                    0_u32,
                )
            };
            if status != 0 {
                return Err(format!(
                    "cannot add Landlock rule for {}: {}",
                    path.display(),
                    std::io::Error::last_os_error()
                ));
            }
        }
        // SAFETY: no-new-privileges is a one-way process restriction.
        if unsafe { prctl(PR_SET_NO_NEW_PRIVS, 1 as c_long, 0, 0, 0) } != 0 {
            return Err(format!(
                "cannot enable no-new-privileges: {}",
                std::io::Error::last_os_error()
            ));
        }
        // SAFETY: the valid ruleset FD is consumed by the kernel for this process.
        if unsafe { syscall(SYS_LANDLOCK_RESTRICT_SELF, ruleset_fd, 0_u32) } != 0 {
            return Err(format!(
                "cannot enforce Landlock ruleset: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    })();
    // SAFETY: ruleset_fd is uniquely owned by this function.
    unsafe { close(ruleset_fd) };
    result
}

#[cfg(not(target_os = "linux"))]
fn apply_filesystem(_read_paths: &[PathBuf], _write_paths: &[PathBuf]) -> Result<(), String> {
    Err("package sandbox currently requires Linux Landlock".to_owned())
}

#[cfg(target_os = "linux")]
fn apply_syscall_filter(allow_network: bool, allow_processes: bool) -> Result<(), String> {
    use std::ffi::c_int;

    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JMP_JEQ_K: u16 = 0x15;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const PR_SET_NO_NEW_PRIVS: c_int = 38;
    const PR_SET_SECCOMP: c_int = 22;
    const SECCOMP_MODE_FILTER: usize = 2;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Filter {
        code: u16,
        jt: u8,
        jf: u8,
        k: u32,
    }

    #[repr(C)]
    struct Program {
        len: u16,
        filter: *const Filter,
    }

    unsafe extern "C" {
        fn prctl(option: c_int, ...) -> c_int;
    }

    let mut denied = Vec::<u32>::new();
    if !allow_processes {
        denied.extend(process_syscalls());
    }
    if !allow_network {
        denied.extend(network_syscalls());
    }
    denied.sort_unstable();
    denied.dedup();
    if denied.is_empty() {
        return Ok(());
    }

    let mut filters = vec![Filter {
        code: BPF_LD_W_ABS,
        jt: 0,
        jf: 0,
        k: 0,
    }];
    for syscall in denied {
        filters.push(Filter {
            code: BPF_JMP_JEQ_K,
            jt: 0,
            jf: 1,
            k: syscall,
        });
        filters.push(Filter {
            code: BPF_RET_K,
            jt: 0,
            jf: 0,
            k: SECCOMP_RET_ERRNO | 1,
        });
    }
    filters.push(Filter {
        code: BPF_RET_K,
        jt: 0,
        jf: 0,
        k: SECCOMP_RET_ALLOW,
    });
    let program = Program {
        len: u16::try_from(filters.len()).map_err(|_| "seccomp filter is too large")?,
        filter: filters.as_ptr(),
    };
    // SAFETY: both prctl calls are one-way restrictions; program and filters
    // remain alive for the synchronous kernel copy.
    if unsafe { prctl(PR_SET_NO_NEW_PRIVS, 1_usize, 0, 0, 0) } != 0 {
        return Err(format!(
            "cannot enable seccomp no-new-privileges: {}",
            std::io::Error::last_os_error()
        ));
    }
    if unsafe { prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &program) } != 0 {
        return Err(format!(
            "cannot install seccomp capability filter: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn apply_syscall_filter(_allow_network: bool, _allow_processes: bool) -> Result<(), String> {
    Err("package sandbox currently requires Linux seccomp".to_owned())
}

#[cfg(target_arch = "x86_64")]
fn process_syscalls() -> Vec<u32> {
    vec![56, 57, 58, 59, 322, 435]
}

#[cfg(target_arch = "aarch64")]
fn process_syscalls() -> Vec<u32> {
    vec![220, 221, 281, 435]
}

#[cfg(target_arch = "x86_64")]
fn network_syscalls() -> Vec<u32> {
    vec![41, 42, 43, 49, 50, 53, 288]
}

#[cfg(target_arch = "aarch64")]
fn network_syscalls() -> Vec<u32> {
    vec![198, 199, 200, 201, 202, 203, 242]
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn process_syscalls() -> Vec<u32> {
    Vec::new()
}

#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
fn network_syscalls() -> Vec<u32> {
    Vec::new()
}
