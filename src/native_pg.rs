//! The Rust side of the `pam_pg_*` PHP functions (`native/pam_pg_php.inc`):
//! Postgres sessions of [`crate::pg_client`], exposed to C as a table of
//! function pointers and addressed from PHP by integer handles.
//!
//! Sessions belong to the worker process that opened them; a forked child
//! sees them as unusable (`pam_pg_status() === -1`) and opens its own.

use std::collections::HashMap;
use std::ffi::{CString, c_char, c_int, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};

use fallible_iterator::FallibleIterator;

use crate::pg_client::{Config, Connection, Param, PgError, QueryResult, TransactionStatus, Value};

const OPS_VERSION: u32 = 1;
/// Open sessions per process; a leak fails loudly instead of exhausting Postgres.
const MAX_SESSIONS: usize = 256;

#[repr(C)]
pub struct PgStr {
    data: *const c_char,
    length: usize,
}

#[repr(C)]
pub struct PgParam {
    /// NULL: positional.
    name: *const c_char,
    name_length: usize,
    value: *const c_char,
    value_length: usize,
    /// 0 NULL, 1 text, 2 binary.
    kind: c_int,
}

#[repr(C)]
pub struct PgColumn {
    name: *const c_char,
    name_length: usize,
    type_oid: u32,
}

#[repr(C)]
pub struct PgCell {
    data: *const u8,
    /// -1: SQL NULL.
    length: isize,
}

#[repr(C)]
pub struct PgResult {
    owner: *mut c_void,
    columns: *const PgColumn,
    column_count: usize,
    cells: *const PgCell,
    row_count: usize,
    affected: u64,
}

#[repr(C)]
pub struct PgErrorOut {
    sqlstate: [c_char; 6],
    message: *mut c_char,
}

#[repr(C)]
pub struct PgOps {
    version: u32,
    connect: unsafe extern "C" fn(*const PgStr, *const PgStr, usize, *mut PgErrorOut) -> u64,
    close: unsafe extern "C" fn(u64),
    execute: unsafe extern "C" fn(
        u64,
        *const c_char,
        usize,
        *const PgParam,
        usize,
        c_int,
        *mut PgResult,
        *mut PgErrorOut,
    ) -> c_int,
    result_free: unsafe extern "C" fn(*mut c_void),
    release: unsafe extern "C" fn(u64, *const c_char, usize),
    transaction: unsafe extern "C" fn(u64, c_int, *mut PgErrorOut) -> c_int,
    status: unsafe extern "C" fn(u64) -> c_int,
    parameter: unsafe extern "C" fn(u64, *const c_char, usize, *mut *mut c_char) -> c_int,
    free_string: unsafe extern "C" fn(*mut c_char),
}

static OPS: PgOps = PgOps {
    version: OPS_VERSION,
    connect,
    close,
    execute,
    result_free,
    release,
    transaction,
    status,
    parameter,
    free_string,
};

unsafe extern "C" {
    fn pam_php_register_pg_ops(ops: *const PgOps);
}

/// Hands the function table to the PHP glue; call before PHP starts.
pub fn register() {
    // SAFETY: OPS is a static with the layout of pam_pg_ops.
    unsafe { pam_php_register_pg_ops(&OPS) };
}

type Session = Arc<Mutex<Connection>>;

static SESSIONS: LazyLock<Mutex<HashMap<u64, Session>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

fn sessions() -> MutexGuard<'static, HashMap<u64, Session>> {
    SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn session(handle: u64) -> Result<Session, PgError> {
    sessions()
        .get(&handle)
        .cloned()
        .ok_or_else(|| PgError::new("08003", "The Postgres session is closed."))
}

fn lock(session: &Session) -> MutexGuard<'_, Connection> {
    session
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Owns everything a [`PgResult`] points into until `result_free`.
struct OwnedResult {
    _result: QueryResult,
    columns: Vec<PgColumn>,
    cells: Vec<PgCell>,
}

fn report(out: *mut PgErrorOut, error: &PgError) {
    if out.is_null() {
        return;
    }
    let mut state = [0 as c_char; 6];
    for (slot, byte) in state.iter_mut().zip(error.sqlstate.bytes().take(5)) {
        *slot = byte as c_char;
    }
    let message = CString::new(error.message.replace('\0', " ")).unwrap_or_default();
    // SAFETY: out is a valid out pointer supplied by the glue.
    unsafe {
        (*out).sqlstate = state;
        (*out).message = message.into_raw();
    }
}

/// # Safety
/// `pointer` must be valid for `length` bytes (or `length` must be 0).
unsafe fn bytes<'a>(pointer: *const c_char, length: usize) -> &'a [u8] {
    if length == 0 || pointer.is_null() {
        return &[];
    }
    // SAFETY: guaranteed by the caller.
    unsafe { std::slice::from_raw_parts(pointer.cast::<u8>(), length) }
}

fn text(bytes: &[u8], what: &str) -> Result<String, PgError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| PgError::new("22021", format!("{what} is not valid UTF-8")))
}

unsafe extern "C" fn connect(
    keys: *const PgStr,
    values: *const PgStr,
    count: usize,
    error: *mut PgErrorOut,
) -> u64 {
    let outcome = (|| {
        let mut pairs = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: the glue passes `count` keys and values.
            let (key, value) = unsafe { (&*keys.add(index), &*values.add(index)) };
            // SAFETY: each PgStr points at a PHP string of that length.
            let key = text(
                unsafe { bytes(key.data, key.length) },
                "a connection option",
            )?;
            let value = text(
                unsafe { bytes(value.data, value.length) },
                "a connection option",
            )?;
            pairs.push((key, value));
        }
        let config = Config::from_pairs(
            pairs
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str())),
        )?;
        if sessions().len() >= MAX_SESSIONS {
            return Err(PgError::new(
                "53300",
                format!("this worker already has {MAX_SESSIONS} open pam-pg sessions"),
            ));
        }
        let connection = Connection::connect(&config)?;
        let handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        sessions().insert(handle, Arc::new(Mutex::new(connection)));
        Ok(handle)
    })();
    outcome.unwrap_or_else(|failure| {
        report(error, &failure);
        0
    })
}

unsafe extern "C" fn close(handle: u64) {
    let removed = sessions().remove(&handle);
    drop(removed);
}

/// `simple` != 0: simple query protocol, no parameters and no rows (PDO::exec).
unsafe extern "C" fn execute(
    handle: u64,
    sql: *const c_char,
    sql_length: usize,
    params: *const PgParam,
    count: usize,
    simple: c_int,
    out: *mut PgResult,
    error: *mut PgErrorOut,
) -> c_int {
    let outcome = (|| {
        // SAFETY: sql points at a PHP string of sql_length bytes.
        let sql = std::str::from_utf8(unsafe { bytes(sql, sql_length) })
            .map_err(|_| PgError::new("22021", "the statement is not valid UTF-8"))?;
        let session = session(handle)?;
        let mut connection = lock(&session);
        if simple != 0 {
            let affected = connection.simple(sql)?;
            return Ok(QueryResult {
                columns: Arc::from(Vec::new()),
                rows: Vec::new(),
                affected,
            });
        }
        let mut bound = Vec::with_capacity(count);
        for index in 0..count {
            // SAFETY: the glue passes `count` parameters that outlive this call.
            let param = unsafe { &*params.add(index) };
            let name = if param.name.is_null() {
                None
            } else {
                // SAFETY: name points at name_length bytes of a PHP string key.
                Some(
                    std::str::from_utf8(unsafe { bytes(param.name, param.name_length) }).map_err(
                        |_| {
                            PgError::new(
                                "HY093",
                                "Invalid parameter number: parameter was not defined",
                            )
                        },
                    )?,
                )
            };
            // SAFETY: value points at value_length bytes of a PHP string.
            let value = unsafe { bytes(param.value, param.value_length) };
            bound.push(Param {
                name,
                value: match param.kind {
                    0 => Value::Null,
                    2 => Value::Binary(value),
                    _ => Value::Text(value),
                },
            });
        }
        connection.execute(sql, &bound)
    })();
    match outcome.and_then(own) {
        Ok(owned) => {
            let owned = Box::new(owned);
            // SAFETY: out is a valid out pointer supplied by the glue.
            unsafe {
                (*out).columns = owned.columns.as_ptr();
                (*out).column_count = owned.columns.len();
                (*out).cells = owned.cells.as_ptr();
                (*out).row_count = owned
                    .cells
                    .len()
                    .checked_div(owned.columns.len())
                    .unwrap_or(0);
                (*out).affected = owned._result.affected;
                (*out).owner = Box::into_raw(owned).cast::<c_void>();
            }
            1
        }
        Err(failure) => {
            report(error, &failure);
            0
        }
    }
}

/// Flattens the rows into cells pointing at the received messages.
fn own(result: QueryResult) -> Result<OwnedResult, PgError> {
    let width = result.columns.len();
    let columns: Vec<PgColumn> = result
        .columns
        .iter()
        .map(|column| PgColumn {
            name: column.name.as_ptr().cast::<c_char>(),
            name_length: column.name.len(),
            type_oid: column.type_oid,
        })
        .collect();
    let mut cells = Vec::with_capacity(width * result.rows.len());
    for row in &result.rows {
        let buffer = row.buffer();
        let mut ranges = row.ranges();
        let mut seen = 0usize;
        while let Some(range) = ranges
            .next()
            .map_err(|error| PgError::new("08P01", format!("Postgres protocol error: {error}")))?
        {
            seen += 1;
            cells.push(match range {
                Some(range) => PgCell {
                    data: buffer[range.clone()].as_ptr(),
                    length: range.len() as isize,
                },
                None => PgCell {
                    data: std::ptr::null(),
                    length: -1,
                },
            });
        }
        if seen != width {
            return Err(PgError::new(
                "08P01",
                "Postgres protocol error: row width does not match its description",
            ));
        }
    }
    Ok(OwnedResult {
        _result: result,
        columns,
        cells,
    })
}

unsafe extern "C" fn result_free(owner: *mut c_void) {
    if !owner.is_null() {
        // SAFETY: owner came from Box::into_raw in execute.
        drop(unsafe { Box::from_raw(owner.cast::<OwnedResult>()) });
    }
}

unsafe extern "C" fn release(handle: u64, sql: *const c_char, sql_length: usize) {
    let Ok(session) = session(handle) else {
        return;
    };
    // SAFETY: sql points at a PHP string of sql_length bytes.
    if let Ok(sql) = std::str::from_utf8(unsafe { bytes(sql, sql_length) }) {
        lock(&session).release(sql);
    }
}

/// 0 begin, 1 commit, 2 rollback.
unsafe extern "C" fn transaction(handle: u64, operation: c_int, error: *mut PgErrorOut) -> c_int {
    let outcome = session(handle).and_then(|session| {
        let mut connection = lock(&session);
        match operation {
            0 => connection.begin(),
            1 => connection.commit(),
            _ => connection.rollback(),
        }
    });
    match outcome {
        Ok(()) => 1,
        Err(failure) => {
            report(error, &failure);
            0
        }
    }
}

/// -1 closed or unusable, 0 idle, 1 in a transaction, 2 in a failed transaction.
unsafe extern "C" fn status(handle: u64) -> c_int {
    let Ok(session) = session(handle) else {
        return -1;
    };
    let connection = lock(&session);
    if !connection.usable() {
        return -1;
    }
    match connection.transaction_status() {
        TransactionStatus::Idle => 0,
        TransactionStatus::InTransaction => 1,
        TransactionStatus::Failed => 2,
    }
}

unsafe extern "C" fn parameter(
    handle: u64,
    name: *const c_char,
    length: usize,
    out: *mut *mut c_char,
) -> c_int {
    let Ok(session) = session(handle) else {
        return 0;
    };
    // SAFETY: name points at a PHP string of `length` bytes.
    let Ok(name) = std::str::from_utf8(unsafe { bytes(name, length) }) else {
        return 0;
    };
    let connection = lock(&session);
    match connection
        .parameter(name)
        .and_then(|value| CString::new(value).ok())
    {
        Some(value) => {
            // SAFETY: out is a valid out pointer supplied by the glue.
            unsafe { *out = value.into_raw() };
            1
        }
        None => 0,
    }
}

unsafe extern "C" fn free_string(value: *mut c_char) {
    if !value.is_null() {
        // SAFETY: every string handed to C came from CString::into_raw.
        drop(unsafe { CString::from_raw(value) });
    }
}
