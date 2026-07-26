use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::p1::{self, WasiP1Ctx};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::{DirPerms, FilePerms, I32Exit, WasiCtxBuilder};

const DEFAULT_FUEL: u64 = 100_000_000;
const DEFAULT_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const DEFAULT_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const MAX_MODULE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;
const MAX_MEMORY_BYTES: usize = 2 * 1024 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_TIMEOUT_MS: u64 = 3_600_000;
const MAX_ENVIRONMENT_ENTRIES: usize = 1_024;
const MAX_ENVIRONMENT_BYTES: usize = 1024 * 1024;
const MAX_GUEST_ARGUMENTS: usize = 1_024;
const MAX_GUEST_ARGUMENT_BYTES: usize = 1024 * 1024;
const MAX_PREOPENED_DIRECTORIES: usize = 64;

#[derive(Clone, Copy)]
#[repr(u8)]
enum DirectoryAccess {
    Read = 1,
    Write = 2,
}

struct DirectoryMapping {
    host: PathBuf,
    guest: String,
    access: DirectoryAccess,
}

struct Options {
    module: PathBuf,
    arguments: Vec<String>,
    input: Vec<u8>,
    environment: Vec<(String, String)>,
    directories: Vec<DirectoryMapping>,
    fuel: u64,
    memory_bytes: usize,
    output_bytes: usize,
    timeout: Duration,
}

struct HostState {
    wasi: WasiP1Ctx,
    limits: StoreLimits,
}

pub(crate) struct Execution {
    pub(crate) status: u8,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

pub fn run(arguments: Vec<OsString>) -> Result<u8, String> {
    let options = Options::parse(arguments)?;
    let execution = execute(options)?;
    std::io::stdout()
        .write_all(&execution.stdout)
        .map_err(|error| format!("cannot write WASI stdout: {error}"))?;
    std::io::stderr()
        .write_all(&execution.stderr)
        .map_err(|error| format!("cannot write WASI stderr: {error}"))?;
    Ok(execution.status)
}

pub(crate) fn execute_rpc(
    module: &Path,
    input: Vec<u8>,
    fuel: u64,
    memory_bytes: usize,
    output_bytes: usize,
    timeout: Duration,
) -> Result<Execution, String> {
    if input.len() as u64 > MAX_INPUT_BYTES {
        return Err(format!(
            "WASI RPC input exceeds the {MAX_INPUT_BYTES} byte limit"
        ));
    }
    if fuel == 0 {
        return Err("WASI RPC fuel must be greater than zero".to_owned());
    }
    if !(64 * 1024..=MAX_MEMORY_BYTES).contains(&memory_bytes) {
        return Err(format!(
            "WASI RPC memory must be between {} and {MAX_MEMORY_BYTES} bytes",
            64 * 1024
        ));
    }
    if !(1..=MAX_OUTPUT_BYTES).contains(&output_bytes) {
        return Err(format!(
            "WASI RPC output must be between 1 and {MAX_OUTPUT_BYTES} bytes"
        ));
    }
    if timeout.is_zero() || timeout > Duration::from_millis(MAX_TIMEOUT_MS) {
        return Err(format!(
            "WASI RPC deadline must be between 1 and {MAX_TIMEOUT_MS} ms"
        ));
    }
    let module = module
        .canonicalize()
        .map_err(|error| format!("cannot resolve WASI module {}: {error}", module.display()))?;
    let module_name = module
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("module.wasm")
        .to_owned();
    execute(Options {
        module,
        arguments: vec![module_name],
        input,
        environment: Vec::new(),
        directories: Vec::new(),
        fuel,
        memory_bytes,
        output_bytes,
        timeout,
    })
}

impl Options {
    fn parse(arguments: Vec<OsString>) -> Result<Self, String> {
        let mut arguments = arguments.into_iter();
        let action = arguments
            .next()
            .ok_or_else(|| "wasi requires `run <module.wasm>`".to_owned())?;
        if action != "run" {
            return Err(format!(
                "unknown WASI action {:?}; expected run",
                action.to_string_lossy()
            ));
        }
        let module = PathBuf::from(
            arguments
                .next()
                .ok_or_else(|| "wasi run requires a WebAssembly module".to_owned())?,
        );
        let mut input = Vec::new();
        let mut environment = Vec::new();
        let mut directories = Vec::new();
        let mut fuel = DEFAULT_FUEL;
        let mut memory_bytes = DEFAULT_MEMORY_BYTES;
        let mut output_bytes = DEFAULT_OUTPUT_BYTES;
        let mut timeout_ms = DEFAULT_TIMEOUT_MS;
        let mut guest_arguments = Vec::new();

        while let Some(argument) = arguments.next() {
            let option = argument.to_string_lossy();
            match option.as_ref() {
                "--" => {
                    guest_arguments.extend(
                        arguments
                            .map(os_string)
                            .collect::<Result<Vec<String>, String>>()?,
                    );
                    break;
                }
                "--stdin" => {
                    let path = PathBuf::from(required_value(&mut arguments, "--stdin")?);
                    input = read_bounded(&path, MAX_INPUT_BYTES, "WASI stdin")?;
                }
                "--env" => {
                    let name = required_value(&mut arguments, "--env")?;
                    validate_environment_name(&name)?;
                    let value = std::env::var(&name)
                        .map_err(|_| format!("WASI environment variable {name} is not set"))?;
                    environment.push((name, value));
                }
                "--read-dir" | "--write-dir" => {
                    let mapping = required_value(&mut arguments, option.as_ref())?;
                    directories.push(parse_directory(
                        &mapping,
                        if option == "--read-dir" {
                            DirectoryAccess::Read
                        } else {
                            DirectoryAccess::Write
                        },
                    )?);
                }
                "--fuel" => {
                    fuel = parse_number(
                        &required_value(&mut arguments, "--fuel")?,
                        "--fuel",
                        1,
                        u64::MAX,
                    )?;
                }
                "--memory-bytes" => {
                    memory_bytes = parse_number(
                        &required_value(&mut arguments, "--memory-bytes")?,
                        "--memory-bytes",
                        64 * 1024,
                        MAX_MEMORY_BYTES,
                    )?;
                }
                "--max-output-bytes" => {
                    output_bytes = parse_number(
                        &required_value(&mut arguments, "--max-output-bytes")?,
                        "--max-output-bytes",
                        1,
                        MAX_OUTPUT_BYTES,
                    )?;
                }
                "--timeout-ms" => {
                    timeout_ms = parse_number(
                        &required_value(&mut arguments, "--timeout-ms")?,
                        "--timeout-ms",
                        1,
                        MAX_TIMEOUT_MS,
                    )?;
                }
                option if option.starts_with('-') => {
                    return Err(format!("unknown wasi run option: {option}"));
                }
                _ => {
                    return Err("guest arguments require `--` before the first argument".to_owned());
                }
            }
        }

        let module = module
            .canonicalize()
            .map_err(|error| format!("cannot resolve WASI module {}: {error}", module.display()))?;
        if !module.is_file() {
            return Err(format!("WASI module is not a file: {}", module.display()));
        }
        let module_size = module
            .metadata()
            .map_err(|error| format!("cannot inspect WASI module: {error}"))?
            .len();
        if module_size > MAX_MODULE_BYTES {
            return Err(format!(
                "WASI module exceeds the {} byte limit",
                MAX_MODULE_BYTES
            ));
        }
        let module_name = module
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("module.wasm")
            .to_owned();
        let mut wasi_arguments = vec![module_name];
        validate_unique_environment(&environment)?;
        if environment.len() > MAX_ENVIRONMENT_ENTRIES
            || environment
                .iter()
                .map(|(name, value)| name.len().saturating_add(value.len()))
                .fold(0_usize, usize::saturating_add)
                > MAX_ENVIRONMENT_BYTES
        {
            return Err(format!(
                "WASI environment exceeds {MAX_ENVIRONMENT_ENTRIES} entries or {MAX_ENVIRONMENT_BYTES} bytes"
            ));
        }
        if guest_arguments.len() > MAX_GUEST_ARGUMENTS
            || guest_arguments
                .iter()
                .map(String::len)
                .fold(0_usize, usize::saturating_add)
                > MAX_GUEST_ARGUMENT_BYTES
        {
            return Err(format!(
                "WASI arguments exceed {MAX_GUEST_ARGUMENTS} entries or {MAX_GUEST_ARGUMENT_BYTES} bytes"
            ));
        }
        validate_unique_directories(&directories)?;
        wasi_arguments.extend(guest_arguments);

        Ok(Self {
            module,
            arguments: wasi_arguments,
            input,
            environment,
            directories,
            fuel,
            memory_bytes,
            output_bytes,
            timeout: Duration::from_millis(timeout_ms),
        })
    }
}

fn validate_unique_environment(environment: &[(String, String)]) -> Result<(), String> {
    let mut names = BTreeSet::new();
    if environment
        .iter()
        .any(|(name, _)| !names.insert(name.as_str()))
    {
        return Err("WASI environment variable grants must be unique".to_owned());
    }
    Ok(())
}

fn validate_unique_directories(directories: &[DirectoryMapping]) -> Result<(), String> {
    if directories.len() > MAX_PREOPENED_DIRECTORIES {
        return Err(format!(
            "WASI supports at most {MAX_PREOPENED_DIRECTORIES} preopened directories"
        ));
    }
    let mut guests = BTreeSet::new();
    if directories
        .iter()
        .any(|directory| !guests.insert(directory.guest.as_str()))
    {
        return Err("WASI guest directory mappings must be unique".to_owned());
    }
    Ok(())
}

fn execute(options: Options) -> Result<Execution, String> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.cranelift_nan_canonicalization(true);
    let engine =
        Engine::new(&config).map_err(|error| format!("cannot initialize WASI: {error}"))?;
    let bytes = read_bounded(&options.module, MAX_MODULE_BYTES, "WASI module")?;
    let module = Module::from_binary(&engine, &bytes)
        .map_err(|error| format!("cannot compile WASI module: {error}"))?;

    let stdin = MemoryInputPipe::new(options.input);
    let capture_bytes = options.output_bytes.saturating_add(1);
    let stdout = MemoryOutputPipe::new(capture_bytes);
    let stderr = MemoryOutputPipe::new(capture_bytes);
    let mut builder = WasiCtxBuilder::new();
    builder
        .stdin(stdin)
        .stdout(stdout.clone())
        .stderr(stderr.clone())
        .args(&options.arguments)
        .allow_blocking_current_thread(true);
    for (name, value) in &options.environment {
        builder.env(name, value);
    }
    for directory in &options.directories {
        let (dir_perms, file_perms) = match directory.access {
            DirectoryAccess::Read => (DirPerms::READ, FilePerms::READ),
            DirectoryAccess::Write => (DirPerms::all(), FilePerms::all()),
        };
        builder
            .preopened_dir(&directory.host, &directory.guest, dir_perms, file_perms)
            .map_err(|error| {
                format!(
                    "cannot preopen WASI directory {} as {}: {error}",
                    directory.host.display(),
                    directory.guest
                )
            })?;
    }

    let limits = StoreLimitsBuilder::new()
        .memory_size(options.memory_bytes)
        .table_elements(100_000)
        .instances(1)
        .tables(16)
        .memories(16)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(
        &engine,
        HostState {
            wasi: builder.build_p1(),
            limits,
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(options.fuel)
        .map_err(|error| format!("cannot configure WASI fuel: {error}"))?;
    store.set_epoch_deadline(1);

    let timed_out = Arc::new(AtomicBool::new(false));
    let timeout_flag = timed_out.clone();
    let timeout_engine = engine.clone();
    let (cancel_timeout, timeout_cancelled) = mpsc::channel();
    let timeout = options.timeout;
    let timeout_thread = std::thread::Builder::new()
        .name("pam-wasi-deadline".to_owned())
        .spawn(move || {
            if timeout_cancelled.recv_timeout(timeout).is_err() {
                timeout_flag.store(true, Ordering::Release);
                timeout_engine.increment_epoch();
            }
        })
        .map_err(|error| format!("cannot start WASI deadline guard: {error}"))?;

    let mut linker: Linker<HostState> = Linker::new(&engine);
    p1::add_to_linker_sync(&mut linker, |state| &mut state.wasi)
        .map_err(|error| format!("cannot link WASI interfaces: {error}"))?;
    let result: wasmtime::Result<()> = (|| {
        let instance = linker.instantiate(&mut store, &module)?;
        let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
        start.call(&mut store, ())
    })();
    let _ = cancel_timeout.send(());
    timeout_thread
        .join()
        .map_err(|_| "WASI deadline guard panicked".to_owned())?;

    let stdout = stdout.contents().to_vec();
    let stderr = stderr.contents().to_vec();
    if stdout.len() > options.output_bytes {
        return Err(format!(
            "WASI stdout exceeded the {} byte limit",
            options.output_bytes
        ));
    }
    if stderr.len() > options.output_bytes {
        return Err(format!(
            "WASI stderr exceeded the {} byte limit",
            options.output_bytes
        ));
    }
    match result {
        Ok(()) => Ok(Execution {
            status: 0,
            stdout,
            stderr,
        }),
        Err(_error) if timed_out.load(Ordering::Acquire) => Err(format!(
            "WASI execution exceeded the {} ms deadline",
            options.timeout.as_millis()
        )),
        Err(error) => {
            let exit = wasi_exit_status(&error);
            if let Some(status) = exit {
                return Ok(Execution {
                    status: u8::try_from(status).unwrap_or(70),
                    stdout,
                    stderr,
                });
            }
            Err(format!("WASI execution failed: {error:#}"))
        }
    }
}

fn wasi_exit_status(error: &wasmtime::Error) -> Option<i32> {
    let mut current: Option<&(dyn std::error::Error + 'static)> =
        Some(AsRef::<dyn std::error::Error>::as_ref(error));

    while let Some(source) = current {
        if let Some(exit) = source.downcast_ref::<I32Exit>() {
            return Some(exit.0);
        }
        current = source.source();
    }

    None
}

fn required_value(
    arguments: &mut impl Iterator<Item = OsString>,
    option: &str,
) -> Result<String, String> {
    arguments
        .next()
        .ok_or_else(|| format!("{option} requires a value"))
        .and_then(os_string)
}

fn os_string(value: OsString) -> Result<String, String> {
    value
        .into_string()
        .map_err(|_| "WASI arguments must be valid UTF-8".to_owned())
}

fn parse_number<T>(value: &str, option: &str, minimum: T, maximum: T) -> Result<T, String>
where
    T: std::str::FromStr + Ord + Copy + std::fmt::Display,
{
    let parsed = value
        .parse::<T>()
        .map_err(|_| format!("{option} requires an integer"))?;
    if parsed < minimum || parsed > maximum {
        return Err(format!("{option} must be between {minimum} and {maximum}"));
    }
    Ok(parsed)
}

fn parse_directory(value: &str, access: DirectoryAccess) -> Result<DirectoryMapping, String> {
    let (host, guest) = value
        .split_once('=')
        .ok_or("WASI directory mappings require HOST=GUEST")?;
    if host.is_empty()
        || guest.is_empty()
        || guest.contains('\0')
        || guest.split('/').any(|part| part == "..")
    {
        return Err("WASI directory mappings require safe HOST=GUEST paths".to_owned());
    }
    let host = Path::new(host)
        .canonicalize()
        .map_err(|error| format!("cannot resolve WASI directory {host}: {error}"))?;
    if !host.is_dir() {
        return Err(format!(
            "WASI preopen is not a directory: {}",
            host.display()
        ));
    }
    Ok(DirectoryMapping {
        host,
        guest: guest.to_owned(),
        access,
    })
}

fn validate_environment_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err("WASI environment names may contain only A-Z, a-z, 0-9 and _".to_owned());
    }
    Ok(())
}

fn read_bounded(path: &Path, limit: u64, label: &str) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path)
        .map_err(|error| format!("cannot open {label} {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("cannot read {label} {}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{label} exceeds the {limit} byte limit"));
    }
    Ok(bytes)
}
