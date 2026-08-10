use std::ffi::{OsStr, OsString, c_char, c_int};
use std::marker::PhantomData;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr;
use std::rc::Rc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, ThreadId};

use serde::Deserialize;

use crate::composer::{self, ComposerProject};

const BOOTSTRAP: &str = include_str!("../runtime/bootstrap.php");
const ASYNC_BOOTSTRAP: &str = include_str!("../runtime/async.php");
const IO_BOOTSTRAP: &str = include_str!("../runtime/io.php");
const STREAMS_BOOTSTRAP: &str = include_str!("../runtime/streams.php");
const HTTP_CLIENT_BOOTSTRAP: &str = include_str!("../runtime/http_client.php");
const TASKS_BOOTSTRAP: &str = include_str!("../runtime/tasks.php");
const WS_BOOTSTRAP: &str = include_str!("../runtime/ws.php");
const OBSERVABILITY_BOOTSTRAP: &str = include_str!("../runtime/observability.php");
const SCOPE_BOOTSTRAP: &str = include_str!("../runtime/scope.php");
const NATIVE_BOOTSTRAP: &str = include_str!("../runtime/native.php");
const DIAGNOSTICS_BOOTSTRAP: &str = include_str!("../runtime/diagnostics.php");
const LARAVEL_BOOTSTRAP: &str = include_str!("../runtime/laravel.php");
const EX_SOFTWARE: c_int = 70;
pub const NATIVE_ABI_VERSION: u32 = 1;

static ACTIVE: AtomicBool = AtomicBool::new(false);
static OWNER: OnceLock<ThreadId> = OnceLock::new();

unsafe extern "C" {
    fn pam_native_abi_version() -> u32;
    fn pam_php_init(argc: c_int, argv: *mut *mut c_char) -> c_int;
    fn pam_php_eval(
        source: *const c_char,
        source_length: usize,
        source_name: *const c_char,
    ) -> c_int;
    fn pam_php_execute_file(filename: *const c_char) -> c_int;
    fn pam_php_server_config(output: *mut *mut c_char, output_length: *mut usize) -> c_int;
    fn pam_php_runtime_info(output: *mut *mut c_char, output_length: *mut usize) -> c_int;
    fn pam_php_runtime_metrics(output: *mut *mut c_char, output_length: *mut usize) -> c_int;
    fn pam_php_runtime_diagnostics(output: *mut *mut c_char, output_length: *mut usize) -> c_int;
    fn pam_php_routes_info(output: *mut *mut c_char, output_length: *mut usize) -> c_int;
    fn pam_php_dispatch_http(
        method: *const c_char,
        method_length: usize,
        target: *const c_char,
        target_length: usize,
        headers: *const c_char,
        headers_length: usize,
        body: *const c_char,
        body_length: usize,
        context: *const c_char,
        context_length: usize,
        request_id: *const c_char,
        request_id_length: usize,
        output: *mut *mut c_char,
        output_length: *mut usize,
    ) -> c_int;
    fn pam_php_resume_http(
        request_id: *const c_char,
        request_id_length: usize,
        body: *const c_char,
        body_length: usize,
        operation_result: *const c_char,
        operation_result_length: usize,
        output: *mut *mut c_char,
        output_length: *mut usize,
    ) -> c_int;
    fn pam_php_cancel_http(request_id: *const c_char, request_id_length: usize) -> c_int;
    fn pam_php_dispatch_ws(
        event_kind: c_int,
        socket_id: *const c_char,
        socket_id_length: usize,
        payload: *const c_char,
        payload_length: usize,
        output: *mut *mut c_char,
        output_length: *mut usize,
    ) -> c_int;
    fn pam_php_string_free(value: *mut c_char);
    fn pam_php_shutdown();
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    #[serde(default = "default_max_body_bytes")]
    pub max_body_bytes: usize,
    #[serde(default = "default_max_header_bytes")]
    pub max_header_bytes: usize,
    #[serde(default = "default_max_headers")]
    pub max_headers: usize,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_max_concurrent_requests")]
    pub max_concurrent_requests: usize,
    #[serde(default = "default_response_stream_queue_capacity")]
    pub response_stream_queue_capacity: usize,
    #[serde(default = "default_max_response_bytes")]
    pub max_response_bytes: usize,
    #[serde(default = "default_max_response_chunk_bytes")]
    pub max_response_chunk_bytes: usize,
    #[serde(default = "default_header_read_timeout_ms")]
    pub header_read_timeout_ms: u64,
    #[serde(default = "default_body_read_timeout_ms")]
    pub body_read_timeout_ms: u64,
    #[serde(default)]
    pub rate_limit_per_second: u32,
    #[serde(default)]
    pub cors_origins: Vec<String>,
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    #[serde(default = "default_metrics_path")]
    pub metrics_path: String,
    #[serde(default)]
    pub tls_cert: Option<String>,
    #[serde(default)]
    pub tls_key: Option<String>,
    #[serde(default = "default_http3")]
    pub http3: bool,
    #[serde(default = "default_websocket_max_connections")]
    pub websocket_max_connections: usize,
    #[serde(default = "default_websocket_max_message_bytes")]
    pub websocket_max_message_bytes: usize,
    #[serde(default = "default_websocket_queue_capacity")]
    pub websocket_queue_capacity: usize,
    #[serde(default = "default_websocket_heartbeat_ms")]
    pub websocket_heartbeat_ms: u64,
    #[serde(default = "default_websocket_timeout_ms")]
    pub websocket_timeout_ms: u64,
    #[serde(default = "default_websocket_compression")]
    pub websocket_compression: bool,
    #[serde(default)]
    pub telemetry_headers: bool,
    #[serde(default)]
    pub access_log: bool,
    #[serde(default = "default_access_log_sample_rate")]
    pub access_log_sample_rate: u64,
    #[serde(default)]
    pub websocket_resume_secret: Option<String>,
    #[serde(default = "default_websocket_resume_ttl_seconds")]
    pub websocket_resume_ttl_seconds: u64,
}

fn default_max_body_bytes() -> usize {
    2 * 1024 * 1024
}
fn default_max_header_bytes() -> usize {
    32 * 1024
}
fn default_max_headers() -> usize {
    100
}
fn default_request_timeout_ms() -> u64 {
    30_000
}
fn default_max_concurrent_requests() -> usize {
    4096
}
fn default_response_stream_queue_capacity() -> usize {
    16
}
fn default_max_response_bytes() -> usize {
    256 * 1024 * 1024
}
fn default_max_response_chunk_bytes() -> usize {
    1024 * 1024
}
fn default_header_read_timeout_ms() -> u64 {
    10_000
}
fn default_body_read_timeout_ms() -> u64 {
    30_000
}
fn default_metrics_path() -> String {
    "/metrics".to_owned()
}
fn default_websocket_max_connections() -> usize {
    10_000
}
fn default_http3() -> bool {
    true
}
fn default_websocket_max_message_bytes() -> usize {
    1024 * 1024
}
fn default_websocket_queue_capacity() -> usize {
    256
}
fn default_websocket_heartbeat_ms() -> u64 {
    15_000
}
fn default_websocket_timeout_ms() -> u64 {
    45_000
}
fn default_websocket_compression() -> bool {
    true
}
fn default_access_log_sample_rate() -> u64 {
    1
}
fn default_websocket_resume_ttl_seconds() -> u64 {
    86_400
}

#[derive(Clone, Debug, Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub php_version: String,
    pub php_version_id: u32,
    pub sapi: String,
    pub zend_version: String,
    pub native_abi_version: u32,
    pub zts: bool,
    pub debug: bool,
    pub integer_size: u8,
    pub ini_loaded: Option<String>,
    pub ini_scanned: Vec<String>,
    pub extensions: Vec<String>,
    pub composer_autoloaded: bool,
    pub xdebug_loaded: bool,
    pub opcache_loaded: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMetrics {
    pub fibers: u64,
    pub memory_bytes: u64,
    pub peak_memory_bytes: u64,
}

pub struct PhpRuntime {
    _native_args: Vec<Vec<u8>>,
    composer: Option<ComposerProject>,
    _not_send_or_sync: PhantomData<Rc<()>>,
}

impl PhpRuntime {
    pub fn initialize(
        _executable: &OsStr,
        script: &Path,
        arguments: &[OsString],
    ) -> Result<Self, String> {
        Self::initialize_with_composer(script, arguments, true)
    }

    pub fn initialize_tool(
        _executable: &OsStr,
        script: &Path,
        arguments: &[OsString],
    ) -> Result<Self, String> {
        Self::initialize_with_composer(script, arguments, false)
    }

    fn initialize_with_composer(
        script: &Path,
        arguments: &[OsString],
        preload_composer: bool,
    ) -> Result<Self, String> {
        // SAFETY: This pure function returns the constant compiled into the C shim.
        let native_abi = unsafe { pam_native_abi_version() };
        if native_abi != NATIVE_ABI_VERSION {
            return Err(format!(
                "Pam Native ABI mismatch: Rust expects {NATIVE_ABI_VERSION}, C shim provides {native_abi}"
            ));
        }
        let current_thread = thread::current().id();
        let owner = OWNER.get_or_init(|| current_thread);
        if *owner != current_thread {
            return Err("PHP must be initialized on its original thread".to_owned());
        }

        let mut native_args = Vec::with_capacity(arguments.len() + 2);
        let executable_path = std::env::current_exe()
            .map_err(|error| format!("cannot locate the Pam executable: {error}"))?;
        native_args.push(to_c_buffer(executable_path.as_os_str())?);
        native_args.push(to_c_buffer(script.as_os_str())?);
        for argument in arguments {
            native_args.push(to_c_buffer(argument)?);
        }

        let mut pointers = native_args
            .iter_mut()
            .map(|argument| argument.as_mut_ptr().cast::<c_char>())
            .collect::<Vec<_>>();
        let argc = c_int::try_from(pointers.len()).map_err(|_| "too many arguments")?;
        let composer = composer::discover(script)?;
        if ACTIVE.swap(true, Ordering::SeqCst) {
            return Err("the PHP runtime is already active".to_owned());
        }

        // SAFETY: Argument buffers are writable and NUL-terminated, and remain owned
        // by PhpRuntime until shutdown.
        let status = unsafe { pam_php_init(argc, pointers.as_mut_ptr()) };

        if status != 0 {
            // The C boundary tracks partial initialization and makes this idempotent.
            unsafe { pam_php_shutdown() };
            ACTIVE.store(false, Ordering::SeqCst);
            return Err(format!("PHP initialization failed with status {status}"));
        }

        if preload_composer
            && let Some(project) = composer.as_ref()
            && project.autoload.is_file()
        {
            let status = match execute_file_raw(&project.autoload) {
                Ok(status) => status,
                Err(error) => {
                    unsafe { pam_php_shutdown() };
                    ACTIVE.store(false, Ordering::SeqCst);
                    return Err(error);
                }
            };
            if status != 0 {
                unsafe { pam_php_shutdown() };
                ACTIVE.store(false, Ordering::SeqCst);
                return Err(format!(
                    "Composer autoload failed with status {status}: {}",
                    project.autoload.display()
                ));
            }
        }

        if let Err(error) = evaluate_runtime_source(ASYNC_BOOTSTRAP, b"Pam async bootstrap\0")
            .and_then(|()| evaluate_runtime_source(IO_BOOTSTRAP, b"Pam I/O bootstrap\0"))
            .and_then(|()| evaluate_runtime_source(STREAMS_BOOTSTRAP, b"Pam streams bootstrap\0"))
            .and_then(|()| {
                evaluate_runtime_source(HTTP_CLIENT_BOOTSTRAP, b"Pam HTTP client bootstrap\0")
            })
            .and_then(|()| evaluate_runtime_source(TASKS_BOOTSTRAP, b"Pam tasks bootstrap\0"))
            .and_then(|()| evaluate_runtime_source(WS_BOOTSTRAP, b"Pam WebSocket bootstrap\0"))
            .and_then(|()| {
                evaluate_runtime_source(OBSERVABILITY_BOOTSTRAP, b"Pam observability bootstrap\0")
            })
            .and_then(|()| {
                evaluate_runtime_source(SCOPE_BOOTSTRAP, b"Pam request scope bootstrap\0")
            })
            .and_then(|()| evaluate_runtime_source(NATIVE_BOOTSTRAP, b"Pam Native API bootstrap\0"))
            .and_then(|()| {
                evaluate_runtime_source(DIAGNOSTICS_BOOTSTRAP, b"Pam diagnostics bootstrap\0")
            })
            .and_then(|()| evaluate_runtime_source(BOOTSTRAP, b"Pam runtime bootstrap\0"))
            .and_then(|()| evaluate_runtime_source(LARAVEL_BOOTSTRAP, b"Pam Laravel bootstrap\0"))
        {
            unsafe { pam_php_shutdown() };
            ACTIVE.store(false, Ordering::SeqCst);
            return Err(error);
        }

        Ok(Self {
            _native_args: native_args,
            composer,
            _not_send_or_sync: PhantomData,
        })
    }

    pub fn execute_file(&mut self, script: &Path) -> Result<u8, String> {
        ensure_owner()?;
        execute_file_raw(script)
    }

    pub fn execute_file_quiet(&mut self, script: &Path) -> Result<u8, String> {
        ensure_owner()?;
        evaluate_runtime_source("<?php ob_start();", b"Pam output capture start\0")?;
        let result = execute_file_raw(script);
        let cleanup = evaluate_runtime_source(
            "<?php if (ob_get_level() > 0) { ob_end_clean(); }",
            b"Pam output capture end\0",
        );
        match (result, cleanup) {
            (Ok(status), Ok(())) => Ok(status),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }

    pub fn execute_code(&mut self, source: &str) -> Result<u8, String> {
        ensure_owner()?;
        let source = source.strip_prefix("<?php").unwrap_or(source);
        // SAFETY: The source buffer remains valid for the synchronous evaluation,
        // and PHP is active on its owner thread.
        let status = unsafe {
            pam_php_eval(
                source.as_ptr().cast::<c_char>(),
                source.len(),
                c"Command line code".as_ptr(),
            )
        };
        Ok(normalize_exit_status(status))
    }

    pub fn server_config(&mut self) -> Result<Option<ServerConfig>, String> {
        ensure_owner()?;
        let json = call_for_string(|output, length| {
            // SAFETY: PHP is active on its owner thread and output pointers are valid.
            unsafe { pam_php_server_config(output, length) }
        })?;

        serde_json::from_str(&json)
            .map_err(|error| format!("invalid server configuration from PHP: {error}"))
    }

    pub fn composer(&self) -> Option<&ComposerProject> {
        self.composer.as_ref()
    }

    pub fn runtime_info(&mut self) -> Result<RuntimeInfo, String> {
        ensure_owner()?;
        let json = call_for_string(|output, length| {
            // SAFETY: PHP is active on its owner thread and output pointers are valid.
            unsafe { pam_php_runtime_info(output, length) }
        })?;

        serde_json::from_str(&json)
            .map_err(|error| format!("invalid runtime information from PHP: {error}"))
    }

    pub fn routes_info(&mut self) -> Result<serde_json::Value, String> {
        ensure_owner()?;
        let json = call_for_string(|output, length| {
            // SAFETY: PHP is active on its owner thread and output pointers are valid.
            unsafe { pam_php_routes_info(output, length) }
        })?;
        serde_json::from_str(&json).map_err(|error| format!("invalid route information: {error}"))
    }

    pub fn runtime_diagnostics(&mut self) -> Result<serde_json::Value, String> {
        ensure_owner()?;
        let json = call_for_string(|output, length| {
            // SAFETY: PHP is active on its owner thread and output pointers are valid.
            unsafe { pam_php_runtime_diagnostics(output, length) }
        })?;
        serde_json::from_str(&json)
            .map_err(|error| format!("invalid runtime diagnostics from PHP: {error}"))
    }
}

impl Drop for PhpRuntime {
    fn drop(&mut self) {
        if ensure_owner().is_ok() {
            // SAFETY: PhpRuntime uniquely owns the active PHP lifecycle.
            unsafe { pam_php_shutdown() };
        }
        ACTIVE.store(false, Ordering::SeqCst);
    }
}

pub fn dispatch_http(
    method: &str,
    target: &str,
    headers_json: &str,
    body: &[u8],
    context_json: &str,
    request_id: &str,
) -> Result<String, String> {
    ensure_owner()?;
    call_for_string(|output, length| {
        // SAFETY: All slices remain valid for the synchronous call; PHP runs only on
        // the owner thread and allocates the returned buffer for call_for_string.
        unsafe {
            pam_php_dispatch_http(
                method.as_ptr().cast::<c_char>(),
                method.len(),
                target.as_ptr().cast::<c_char>(),
                target.len(),
                headers_json.as_ptr().cast::<c_char>(),
                headers_json.len(),
                body.as_ptr().cast::<c_char>(),
                body.len(),
                context_json.as_ptr().cast::<c_char>(),
                context_json.len(),
                request_id.as_ptr().cast::<c_char>(),
                request_id.len(),
                output,
                length,
            )
        }
    })
}

pub fn resume_http(
    request_id: &str,
    body: &[u8],
    operation_result: &str,
) -> Result<String, String> {
    ensure_owner()?;
    call_for_string(|output, length| {
        // SAFETY: Buffers remain valid for this synchronous owner-thread call.
        unsafe {
            pam_php_resume_http(
                request_id.as_ptr().cast::<c_char>(),
                request_id.len(),
                body.as_ptr().cast::<c_char>(),
                body.len(),
                operation_result.as_ptr().cast::<c_char>(),
                operation_result.len(),
                output,
                length,
            )
        }
    })
}

pub fn cancel_http(request_id: &str) -> Result<(), String> {
    ensure_owner()?;
    // SAFETY: The request ID remains valid for this synchronous owner-thread call.
    let status =
        unsafe { pam_php_cancel_http(request_id.as_ptr().cast::<c_char>(), request_id.len()) };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("PHP HTTP cancellation failed with status {status}"))
    }
}

pub fn runtime_metrics() -> Result<RuntimeMetrics, String> {
    ensure_owner()?;
    let json = call_for_string(|output, length| {
        // SAFETY: PHP is active on its owner thread and output pointers are valid.
        unsafe { pam_php_runtime_metrics(output, length) }
    })?;
    serde_json::from_str(&json).map_err(|error| format!("invalid PHP runtime metrics: {error}"))
}

pub fn dispatch_ws(event_kind: i32, socket_id: &str, payload: &str) -> Result<String, String> {
    ensure_owner()?;
    call_for_string(|output, length| {
        // SAFETY: All strings remain valid for this synchronous owner-thread call.
        unsafe {
            pam_php_dispatch_ws(
                event_kind,
                socket_id.as_ptr().cast::<c_char>(),
                socket_id.len(),
                payload.as_ptr().cast::<c_char>(),
                payload.len(),
                output,
                length,
            )
        }
    })
}

fn call_for_string(
    call: impl FnOnce(*mut *mut c_char, *mut usize) -> c_int,
) -> Result<String, String> {
    let mut output = ptr::null_mut();
    let mut output_length = 0;
    let status = call(&mut output, &mut output_length);

    if status != 0 || output.is_null() {
        return Err(format!("PHP dispatch failed with status {status}"));
    }

    // SAFETY: A successful C call returns an allocation of output_length bytes.
    let bytes = unsafe { std::slice::from_raw_parts(output.cast::<u8>(), output_length) };
    let result = String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("PHP returned invalid UTF-8: {error}"));

    // SAFETY: The buffer came from the matching C allocation function.
    unsafe { pam_php_string_free(output) };
    result
}

fn ensure_owner() -> Result<(), String> {
    if !ACTIVE.load(Ordering::SeqCst) {
        return Err("the PHP runtime is not active".to_owned());
    }

    if OWNER
        .get()
        .is_none_or(|owner| *owner != thread::current().id())
    {
        return Err("PHP dispatch attempted from a non-owner thread".to_owned());
    }

    Ok(())
}

fn execute_file_raw(script: &Path) -> Result<u8, String> {
    let filename = to_c_buffer(script.as_os_str())?;

    // SAFETY: The filename is NUL-terminated and PHP is active on this thread.
    let status = unsafe { pam_php_execute_file(filename.as_ptr().cast::<c_char>()) };
    Ok(normalize_exit_status(status))
}

fn evaluate_runtime_source(source: &str, source_name: &'static [u8]) -> Result<(), String> {
    let source = source
        .strip_prefix("<?php")
        .ok_or("a PHP runtime bootstrap must start with <?php")?;

    // SAFETY: Both source buffers remain valid for the synchronous evaluation;
    // source_name is a static, NUL-terminated byte string.
    let status = unsafe {
        pam_php_eval(
            source.as_ptr().cast::<c_char>(),
            source.len(),
            source_name.as_ptr().cast::<c_char>(),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(format!("Pam bootstrap failed with status {status}"))
    }
}

fn normalize_exit_status(status: c_int) -> u8 {
    if (0..=u8::MAX as c_int).contains(&status) {
        status as u8
    } else {
        EX_SOFTWARE as u8
    }
}

fn to_c_buffer(value: &OsStr) -> Result<Vec<u8>, String> {
    let bytes = value.as_bytes();
    if bytes.contains(&0) {
        return Err("arguments cannot contain a NUL byte".to_owned());
    }

    let mut buffer = Vec::with_capacity(bytes.len() + 1);
    buffer.extend_from_slice(bytes);
    buffer.push(0);
    Ok(buffer)
}
