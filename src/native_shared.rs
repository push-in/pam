//! The Rust side of the PHP functions in `native/pam_shared_php.inc`:
//! shared tables (`crate::shared_table`) and task workers
//! (`crate::task_workers`), exposed to C as a table of function pointers.

use std::ffi::{CString, c_char, c_int, c_void};
use std::time::Duration;

use bytes::Bytes;

use crate::shared_table::{self, Layout, Patch, Table, Write};
use crate::task_workers;

const OPS_VERSION: u32 = 1;

#[repr(C)]
pub struct SharedPatch {
    offset: u32,
    length: u32,
    bytes: *const u8,
}

#[repr(C)]
pub struct SharedBytes {
    data: *const u8,
    length: usize,
}

#[repr(C)]
pub struct SharedOps {
    version: u32,
    table_open: unsafe extern "C" fn(
        *const c_char,
        usize,
        u64,
        u32,
        u32,
        u64,
        *mut *mut c_char,
    ) -> *mut c_void,
    table_close: unsafe extern "C" fn(*mut c_void),
    table_set: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        usize,
        *const SharedPatch,
        usize,
        *mut *mut c_char,
    ) -> c_int,
    table_get: unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut u8, usize) -> c_int,
    table_exists: unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> c_int,
    table_del: unsafe extern "C" fn(*mut c_void, *const c_char, usize) -> c_int,
    table_count: unsafe extern "C" fn(*mut c_void) -> u64,
    #[allow(clippy::type_complexity)]
    table_incr: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        usize,
        u32,
        c_int,
        i64,
        f64,
        *mut i64,
        *mut f64,
        *mut *mut c_char,
    ) -> c_int,
    table_scan:
        unsafe extern "C" fn(*mut c_void, *mut u64, usize, *mut *mut u8, *mut usize) -> usize,
    table_info: unsafe extern "C" fn(*mut c_void, *mut u64, *mut u64, *mut u64),
    task_available: unsafe extern "C" fn() -> c_int,
    task_resolve: unsafe extern "C" fn(
        *const SharedBytes,
        usize,
        u64,
        *mut SharedBytes,
        *mut *mut c_char,
    ) -> c_int,
    task_dispatch: unsafe extern "C" fn(*const SharedBytes, usize, *mut *mut c_char) -> c_int,
    task_flush: unsafe extern "C" fn(u64) -> usize,
    free_buffer: unsafe extern "C" fn(*mut u8, usize),
    free_string: unsafe extern "C" fn(*mut c_char),
}

static OPS: SharedOps = SharedOps {
    version: OPS_VERSION,
    table_open,
    table_close,
    table_set,
    table_get,
    table_exists,
    table_del,
    table_count,
    table_incr,
    table_scan,
    table_info,
    task_available,
    task_resolve,
    task_dispatch,
    task_flush,
    free_buffer,
    free_string,
};

unsafe extern "C" {
    fn pam_php_register_shared_ops(ops: *const SharedOps);
}

/// Hands the function table to the PHP glue; call before PHP starts.
pub fn register() {
    // SAFETY: OPS is a static with the layout of pam_shared_ops.
    unsafe { pam_php_register_shared_ops(&OPS) };
}

fn report(error: *mut *mut c_char, message: String) {
    if error.is_null() {
        return;
    }
    let message = CString::new(message.replace('\0', " ")).unwrap_or_default();
    // SAFETY: error is a valid out pointer supplied by the glue.
    unsafe { *error = message.into_raw() };
}

/// # Safety
/// `pointer` must be valid for `length` bytes (or `length` must be 0).
unsafe fn slice<'a>(pointer: *const c_char, length: usize) -> &'a [u8] {
    if length == 0 || pointer.is_null() {
        return &[];
    }
    // SAFETY: guaranteed by the caller.
    unsafe { std::slice::from_raw_parts(pointer.cast(), length) }
}

/// # Safety
/// `table` must come from `table_open` and not be closed.
unsafe fn table<'a>(table: *mut c_void) -> &'a Table {
    // SAFETY: guaranteed by the caller.
    unsafe { &*(table as *const Table) }
}

unsafe extern "C" fn table_open(
    name: *const c_char,
    name_length: usize,
    rows: u64,
    key_size: u32,
    value_size: u32,
    schema: u64,
    error: *mut *mut c_char,
) -> *mut c_void {
    // SAFETY: the glue passes a PHP string.
    let name = match std::str::from_utf8(unsafe { slice(name, name_length) }) {
        Ok(name) => name,
        Err(_) => {
            report(error, "table names must be ASCII".to_owned());
            return std::ptr::null_mut();
        }
    };
    match Layout::new(rows, key_size, value_size, schema)
        .and_then(|layout| shared_table::open(name, layout))
    {
        Ok(table) => Box::into_raw(Box::new(table)).cast(),
        Err(message) => {
            report(error, message);
            std::ptr::null_mut()
        }
    }
}

unsafe extern "C" fn table_close(table: *mut c_void) {
    if !table.is_null() {
        // SAFETY: created by Box::into_raw in table_open.
        drop(unsafe { Box::from_raw(table as *mut Table) });
    }
}

unsafe extern "C" fn table_set(
    handle: *mut c_void,
    key: *const c_char,
    key_length: usize,
    patches: *const SharedPatch,
    count: usize,
    error: *mut *mut c_char,
) -> c_int {
    // SAFETY: the glue passes `count` patches pointing into its scratch row.
    let raw = if count == 0 {
        &[][..]
    } else {
        unsafe { std::slice::from_raw_parts(patches, count) }
    };
    let patches: Vec<Patch<'_>> = raw
        .iter()
        .map(|patch| Patch {
            offset: patch.offset,
            // SAFETY: each patch covers `length` bytes of the scratch row.
            bytes: unsafe { slice(patch.bytes.cast(), patch.length as usize) },
        })
        .collect();
    // SAFETY: handle and key come from the glue.
    match unsafe { table(handle) }.set(unsafe { slice(key, key_length) }, &patches) {
        Ok(Write::Stored) => 1,
        Ok(Write::Full) => 0,
        Err(message) => {
            report(error, message);
            -1
        }
    }
}

unsafe extern "C" fn table_get(
    handle: *mut c_void,
    key: *const c_char,
    key_length: usize,
    output: *mut u8,
    output_length: usize,
) -> c_int {
    // SAFETY: output holds output_length bytes.
    let output = unsafe { std::slice::from_raw_parts_mut(output, output_length) };
    // SAFETY: handle and key come from the glue.
    c_int::from(unsafe { table(handle) }.get(unsafe { slice(key, key_length) }, output))
}

unsafe extern "C" fn table_exists(
    handle: *mut c_void,
    key: *const c_char,
    key_length: usize,
) -> c_int {
    // SAFETY: handle and key come from the glue.
    c_int::from(unsafe { table(handle) }.exists(unsafe { slice(key, key_length) }))
}

unsafe extern "C" fn table_del(
    handle: *mut c_void,
    key: *const c_char,
    key_length: usize,
) -> c_int {
    // SAFETY: handle and key come from the glue.
    c_int::from(unsafe { table(handle) }.delete(unsafe { slice(key, key_length) }))
}

unsafe extern "C" fn table_count(handle: *mut c_void) -> u64 {
    // SAFETY: handle comes from the glue.
    unsafe { table(handle) }.count()
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn table_incr(
    handle: *mut c_void,
    key: *const c_char,
    key_length: usize,
    offset: u32,
    is_float: c_int,
    int_delta: i64,
    float_delta: f64,
    int_result: *mut i64,
    float_result: *mut f64,
    error: *mut *mut c_char,
) -> c_int {
    // SAFETY: handle and key come from the glue; results are valid out pointers.
    let table = unsafe { table(handle) };
    let key = unsafe { slice(key, key_length) };
    let outcome = if is_float != 0 {
        table
            .increment_float(key, offset, float_delta)
            .map(|value| value.map(|value| unsafe { *float_result = value }))
    } else {
        table
            .increment_int(key, offset, int_delta)
            .map(|value| value.map(|value| unsafe { *int_result = value }))
    };
    match outcome {
        Ok(Some(())) => 1,
        Ok(None) => 0,
        Err(message) => {
            report(error, message);
            -1
        }
    }
}

unsafe extern "C" fn table_scan(
    handle: *mut c_void,
    cursor: *mut u64,
    limit: usize,
    output: *mut *mut u8,
    output_length: *mut usize,
) -> usize {
    let mut buffer = Vec::new();
    // SAFETY: valid pointers from the glue.
    let rows = unsafe { table(handle).scan(&mut *cursor, limit, &mut buffer) };
    let boxed = buffer.into_boxed_slice();
    // SAFETY: out pointers from the glue; freed with free_buffer.
    unsafe {
        *output_length = boxed.len();
        *output = Box::into_raw(boxed).cast();
    }
    rows
}

unsafe extern "C" fn table_info(
    handle: *mut c_void,
    capacity: *mut u64,
    memory: *mut u64,
    recoveries: *mut u64,
) {
    // SAFETY: valid pointers from the glue.
    unsafe {
        let table = table(handle);
        *capacity = u64::from(table.layout().capacity);
        *memory = table.memory_size() as u64;
        *recoveries = table.recoveries();
    }
}

unsafe extern "C" fn task_available() -> c_int {
    c_int::from(endpoint().is_some())
}

fn endpoint() -> Option<&'static task_workers::Endpoint> {
    static ENDPOINT: std::sync::OnceLock<Option<task_workers::Endpoint>> =
        std::sync::OnceLock::new();
    ENDPOINT
        .get_or_init(|| {
            // Task workers never send tasks to themselves (no self-deadlock).
            if std::env::var_os(task_workers::ROLE_ENV).is_some() {
                None
            } else {
                task_workers::Endpoint::from_environment()
            }
        })
        .as_ref()
}

/// # Safety
/// `payloads` must hold `count` valid byte ranges.
unsafe fn payloads(payloads: *const SharedBytes, count: usize) -> Vec<Bytes> {
    if count == 0 {
        return Vec::new();
    }
    // SAFETY: guaranteed by the caller.
    unsafe { std::slice::from_raw_parts(payloads, count) }
        .iter()
        // SAFETY: each entry points at a PHP string.
        .map(|payload| {
            Bytes::copy_from_slice(unsafe { slice(payload.data.cast(), payload.length) })
        })
        .collect()
}

unsafe extern "C" fn task_resolve(
    input: *const SharedBytes,
    count: usize,
    wait_ms: u64,
    results: *mut SharedBytes,
    error: *mut *mut c_char,
) -> c_int {
    let Some(endpoint) = endpoint() else {
        report(
            error,
            "this PAM cluster has no task workers (--task-workers)".to_owned(),
        );
        return -1;
    };
    // SAFETY: the glue passes `count` payloads.
    let outcomes = match task_workers::resolve(
        endpoint,
        unsafe { payloads(input, count) },
        Duration::from_millis(wait_ms),
    ) {
        Ok(outcomes) => outcomes,
        Err(message) => {
            report(error, message);
            return -1;
        }
    };
    let mut failure = None;
    let mut converted = Vec::with_capacity(count);
    for outcome in outcomes {
        match outcome {
            task_workers::Outcome::Finished(body) => {
                let boxed: Box<[u8]> = body.to_vec().into_boxed_slice();
                let length = boxed.len();
                converted.push(SharedBytes {
                    data: Box::into_raw(boxed).cast::<u8>(),
                    length,
                });
            }
            task_workers::Outcome::Pending => converted.push(SharedBytes {
                data: std::ptr::null(),
                length: 0,
            }),
            task_workers::Outcome::Failed(message) => {
                failure.get_or_insert(message);
                converted.push(SharedBytes {
                    data: std::ptr::null(),
                    length: 0,
                });
            }
        }
    }
    if let Some(message) = failure {
        for entry in converted {
            // SAFETY: allocated above.
            unsafe { free_buffer(entry.data as *mut u8, entry.length) };
        }
        report(error, message);
        return -1;
    }
    for (index, entry) in converted.into_iter().enumerate() {
        // SAFETY: results holds `count` entries.
        unsafe { *results.add(index) = entry };
    }
    0
}

unsafe extern "C" fn task_dispatch(
    input: *const SharedBytes,
    count: usize,
    error: *mut *mut c_char,
) -> c_int {
    let Some(endpoint) = endpoint() else {
        report(
            error,
            "this PAM cluster has no task workers (--task-workers)".to_owned(),
        );
        return -1;
    };
    // SAFETY: the glue passes `count` payloads.
    match task_workers::dispatch(endpoint, unsafe { payloads(input, count) }) {
        Ok(()) => 0,
        Err(message) => {
            report(error, message);
            -1
        }
    }
}

unsafe extern "C" fn task_flush(timeout_ms: u64) -> usize {
    task_workers::flush(Duration::from_millis(timeout_ms))
}

unsafe extern "C" fn free_buffer(data: *mut u8, length: usize) {
    if !data.is_null() {
        // SAFETY: allocated as a Box<[u8]> of `length` bytes.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, length)) });
    }
}

unsafe extern "C" fn free_string(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: allocated by CString::into_raw in report().
        drop(unsafe { CString::from_raw(value) });
    }
}
