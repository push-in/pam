//! A blocking Postgres client for the PHP worker thread (`pam-pg`).
//!
//! The PHP worker calls it synchronously, like PDO: the thread sleeps in
//! `read(2)` on the session socket while Postgres works, so a query costs no
//! Fiber suspension and no hand-off to another thread. Messages are encoded
//! and decoded by `postgres-protocol` (the codec of `tokio-postgres`); this
//! module only sequences them.
//!
//! - Every statement goes through the extended protocol with its values bound
//!   as parameters (text format, or binary for large objects); the SQL text is
//!   never built from values, so a value cannot change the statement.
//! - Statements are parsed once per session and executed by name afterwards
//!   (`Parse`/`Describe` the first time, `Bind`/`Execute`/`Sync` after that), in
//!   one write and one round trip each time.
//! - `begin()` is sent together with the first statement of the transaction,
//!   in the same pipeline (an error in `BEGIN` skips the statement too, as if it
//!   had been sent on its own); `BEGIN` followed by `COMMIT` with nothing in
//!   between sends nothing.
//! - Results come back in text format, exactly the representation libpq and
//!   pdo_pgsql see.

use std::collections::HashMap;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::{BufMut, BytesMut};
use fallible_iterator::FallibleIterator;
use postgres_protocol::IsNull;
use postgres_protocol::authentication::sasl::{self, ChannelBinding, ScramSha256};
use postgres_protocol::message::backend::{DataRowBody, ErrorFields, Message};
use postgres_protocol::message::frontend;

/// Statements kept prepared per session; past it the cache is closed and rebuilt.
const STATEMENT_CACHE_LIMIT: usize = 512;
/// Read buffer capacity kept between queries; larger results release theirs.
const READ_BUFFER_KEEP: usize = 1 << 20;
const READ_CHUNK: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PgError {
    /// Five character SQLSTATE.
    pub sqlstate: String,
    pub message: String,
}

impl PgError {
    pub fn new(sqlstate: &str, message: impl Into<String>) -> Self {
        Self {
            sqlstate: sqlstate.to_owned(),
            message: message.into(),
        }
    }

    fn io(error: &io::Error) -> Self {
        Self::new("08006", format!("Postgres connection lost: {error}"))
    }

    /// From an `ErrorResponse`: `SEVERITY:  message`, plus detail and hint lines.
    fn from_fields(mut fields: ErrorFields<'_>) -> Self {
        let mut sqlstate = String::from("XX000");
        let mut severity = String::new();
        let mut message = String::new();
        let mut detail = String::new();
        let mut hint = String::new();
        while let Ok(Some(field)) = fields.next() {
            let value = String::from_utf8_lossy(field.value_bytes()).into_owned();
            match field.type_() {
                b'C' => sqlstate = value,
                b'V' => severity = value,
                b'S' if severity.is_empty() => severity = value,
                b'M' => message = value,
                b'D' => detail = value,
                b'H' => hint = value,
                _ => {}
            }
        }
        let mut text = if severity.is_empty() {
            message
        } else {
            format!("{severity}:  {message}")
        };
        if !detail.is_empty() {
            text.push_str("\nDETAIL:  ");
            text.push_str(&detail);
        }
        if !hint.is_empty() {
            text.push_str("\nHINT:  ");
            text.push_str(&hint);
        }
        Self::new(&sqlstate, text)
    }
}

impl fmt::Display for PgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "SQLSTATE[{}]: {}", self.sqlstate, self.message)
    }
}

/// Connection settings, from libpq keywords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub hosts: Vec<String>,
    pub ports: Vec<u16>,
    pub dbname: Option<String>,
    pub user: String,
    pub password: Option<String>,
    pub application_name: Option<String>,
    pub options: Option<String>,
    pub connect_timeout: Option<Duration>,
    pub ssl_mode: SslMode,
    pub ssl_root_cert: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    Disable,
    Allow,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl Config {
    /// libpq keywords (`host`, `port`, `dbname`, `user`, `password`,
    /// `sslmode`, ...). Unknown or unsupported keywords are an error rather
    /// than silently ignored.
    pub fn from_pairs<'a>(
        pairs: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, PgError> {
        let mut config = Config {
            hosts: Vec::new(),
            ports: Vec::new(),
            dbname: None,
            user: String::new(),
            password: None,
            application_name: None,
            options: None,
            connect_timeout: None,
            ssl_mode: SslMode::Prefer,
            ssl_root_cert: None,
        };
        let invalid = |message: String| PgError::new("08001", message);
        for (key, value) in pairs {
            match key {
                "host" | "hostaddr" => {
                    config.hosts = value
                        .split(',')
                        .map(str::trim)
                        .filter(|host| !host.is_empty())
                        .map(str::to_owned)
                        .collect();
                }
                "port" => {
                    config.ports = value
                        .split(',')
                        .map(|port| port.trim().parse::<u16>())
                        .collect::<Result<_, _>>()
                        .map_err(|_| invalid(format!("invalid port \"{value}\"")))?;
                }
                "dbname" => config.dbname = Some(value.to_owned()),
                "user" => config.user = value.to_owned(),
                "password" => config.password = Some(value.to_owned()),
                "application_name" => config.application_name = Some(value.to_owned()),
                "options" => config.options = Some(value.to_owned()),
                "connect_timeout" => {
                    let seconds = value
                        .trim()
                        .parse::<u64>()
                        .map_err(|_| invalid(format!("invalid connect_timeout \"{value}\"")))?;
                    config.connect_timeout =
                        (seconds > 0).then(|| Duration::from_secs(seconds.max(2)));
                }
                "sslmode" => {
                    config.ssl_mode = match value {
                        "disable" => SslMode::Disable,
                        "allow" => SslMode::Allow,
                        "prefer" => SslMode::Prefer,
                        "require" => SslMode::Require,
                        "verify-ca" => SslMode::VerifyCa,
                        "verify-full" => SslMode::VerifyFull,
                        _ => return Err(invalid(format!("invalid sslmode \"{value}\""))),
                    };
                }
                "sslrootcert" => config.ssl_root_cert = Some(value.to_owned()),
                // Accepted and meaningless here: the client always speaks UTF-8.
                "client_encoding"
                    if value.eq_ignore_ascii_case("utf8")
                        || value.eq_ignore_ascii_case("utf-8") => {}
                _ => {
                    return Err(invalid(format!(
                        "the pam-pg driver does not support the \"{key}\" connection option; use the PDO driver"
                    )));
                }
            }
        }
        if config.user.is_empty() {
            config.user = std::env::var("PGUSER")
                .or_else(|_| std::env::var("USER"))
                .unwrap_or_else(|_| "postgres".to_owned());
        }
        if config.hosts.is_empty() {
            config.hosts.push(if cfg!(unix) {
                "/tmp".to_owned()
            } else {
                "localhost".to_owned()
            });
        }
        if config.ports.len() > 1 && config.ports.len() != config.hosts.len() {
            return Err(invalid(
                "the number of ports must match the number of hosts".to_owned(),
            ));
        }
        Ok(config)
    }

    fn port(&self, index: usize) -> u16 {
        match self.ports.as_slice() {
            [] => 5432,
            [port] => *port,
            ports => ports[index],
        }
    }
}

enum Stream {
    Tcp(TcpStream),
    #[cfg(unix)]
    Unix(std::os::unix::net::UnixStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Stream {
    fn set_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        match self {
            Stream::Tcp(stream) => {
                stream.set_read_timeout(timeout)?;
                stream.set_write_timeout(timeout)
            }
            #[cfg(unix)]
            Stream::Unix(stream) => {
                stream.set_read_timeout(timeout)?;
                stream.set_write_timeout(timeout)
            }
            Stream::Tls(stream) => {
                stream.sock.set_read_timeout(timeout)?;
                stream.sock.set_write_timeout(timeout)
            }
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(stream) => stream.read(buffer),
            #[cfg(unix)]
            Stream::Unix(stream) => stream.read(buffer),
            Stream::Tls(stream) => stream.read(buffer),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Stream::Tcp(stream) => stream.write(buffer),
            #[cfg(unix)]
            Stream::Unix(stream) => stream.write(buffer),
            Stream::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Stream::Tcp(stream) => stream.flush(),
            #[cfg(unix)]
            Stream::Unix(stream) => stream.flush(),
            Stream::Tls(stream) => stream.flush(),
        }
    }
}

/// How a statement names its placeholders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placeholders {
    /// `?` (or none): the values in order.
    Positional(usize),
    /// `:name`: one `$n` per distinct name, in order of first use.
    Named(Vec<String>),
}

/// Rewrites PDO placeholders into Postgres `$n` ones, with pdo_pgsql's
/// rules: `?` and `:name` outside string literals (standard, `E''` and
/// dollar quoted), quoted identifiers and comments; `??` is a literal `?`
/// and `::` a cast. Mixing `?` and `:name` is an error, as in PDO.
pub fn rewrite_placeholders(sql: &str) -> Result<(String, Placeholders), PgError> {
    let bytes = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut positional = 0usize;
    let mut names: Vec<String> = Vec::new();
    let mut copied = 0usize;
    let mut index = 0usize;
    let ident = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\'' => {
                let escapes = index > 0
                    && matches!(bytes[index - 1], b'E' | b'e')
                    && (index < 2 || !ident(bytes[index - 2]));
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' if escapes => index += 2,
                        b'\'' if bytes.get(index + 1) == Some(&b'\'') => index += 2,
                        b'\'' => break,
                        _ => index += 1,
                    }
                }
                index += 1;
            }
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'"' {
                        if bytes.get(index + 1) == Some(&b'"') {
                            index += 2;
                            continue;
                        }
                        break;
                    }
                    index += 1;
                }
                index += 1;
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index = memchr::memchr(b'\n', &bytes[index..]).map_or(bytes.len(), |at| index + at);
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let mut depth = 1usize;
                index += 2;
                while index < bytes.len() && depth > 0 {
                    if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                        depth += 1;
                        index += 2;
                    } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                        depth -= 1;
                        index += 2;
                    } else {
                        index += 1;
                    }
                }
            }
            b'$' if index == 0 || !ident(bytes[index - 1]) => {
                // $tag$ ... $tag$ (a digit after $ is a parameter reference).
                let start = index;
                let mut end = index + 1;
                while end < bytes.len()
                    && (ident(bytes[end]) && !(end == index + 1 && bytes[end].is_ascii_digit()))
                {
                    end += 1;
                }
                if end < bytes.len() && bytes[end] == b'$' {
                    let tag = &sql[start..=end];
                    index = match sql[end + 1..].find(tag) {
                        Some(at) => end + 1 + at + tag.len(),
                        None => bytes.len(),
                    };
                } else {
                    index += 1;
                }
            }
            b'?' => {
                out.push_str(&sql[copied..index]);
                if bytes.get(index + 1) == Some(&b'?') {
                    out.push('?');
                    index += 2;
                } else {
                    if !names.is_empty() {
                        return Err(mixed_placeholders());
                    }
                    positional += 1;
                    out.push('$');
                    out.push_str(&positional.to_string());
                    index += 1;
                }
                copied = index;
            }
            b':' if bytes.get(index + 1) == Some(&b':') => index += 2,
            b':' if bytes
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphanumeric() || *next == b'_') =>
            {
                let start = index + 1;
                let mut end = start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                if positional > 0 {
                    return Err(mixed_placeholders());
                }
                let name = &sql[start..end];
                let number = match names.iter().position(|known| known == name) {
                    Some(at) => at + 1,
                    None => {
                        names.push(name.to_owned());
                        names.len()
                    }
                };
                out.push_str(&sql[copied..index]);
                out.push('$');
                out.push_str(&number.to_string());
                index = end;
                copied = index;
            }
            _ => index += 1,
        }
    }
    if copied == 0 {
        return Ok((sql.to_owned(), Placeholders::Positional(0)));
    }
    out.push_str(&sql[copied.min(sql.len())..]);
    let placeholders = if names.is_empty() {
        Placeholders::Positional(positional)
    } else {
        Placeholders::Named(names)
    };
    Ok((out, placeholders))
}

fn mixed_placeholders() -> PgError {
    PgError::new(
        "HY093",
        "Invalid parameter number: mixed named and positional parameters",
    )
}

#[derive(Debug)]
pub struct Column {
    pub name: String,
    pub type_oid: u32,
}

struct Statement {
    name: Arc<str>,
    placeholders: Arc<Placeholders>,
    columns: Arc<[Column]>,
}

/// One bound value.
#[derive(Debug, Clone, Copy)]
pub enum Value<'a> {
    Null,
    Text(&'a [u8]),
    Binary(&'a [u8]),
}

/// A value and, for `:name` placeholders, its name (without the colon).
#[derive(Debug, Clone, Copy)]
pub struct Param<'a> {
    pub name: Option<&'a str>,
    pub value: Value<'a>,
}

pub struct QueryResult {
    pub columns: Arc<[Column]>,
    pub rows: Vec<DataRowBody>,
    pub affected: u64,
}

impl fmt::Debug for QueryResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QueryResult")
            .field("columns", &self.columns)
            .field("rows", &self.rows.len())
            .field("affected", &self.affected)
            .finish()
    }
}

impl QueryResult {
    fn empty() -> Self {
        Self {
            columns: Arc::from(Vec::new()),
            rows: Vec::new(),
            affected: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionStatus {
    Idle,
    InTransaction,
    Failed,
}

pub struct Connection {
    stream: Stream,
    read: BytesMut,
    write: BytesMut,
    /// By SQL text; FxHash: the key is hashed on every execution.
    statements: rustc_hash::FxHashMap<Box<str>, Statement>,
    next_statement: u64,
    /// Server-side statements to close before the next command.
    closing: Vec<Arc<str>>,
    /// `ReadyForQuery` answers of close groups still to read.
    close_syncs: usize,
    status: u8,
    pending_begin: bool,
    broken: bool,
    /// The process that opened the session; a forked child must not use it.
    owner: u32,
    parameters: HashMap<String, String>,
}

impl Connection {
    pub fn connect(config: &Config) -> Result<Self, PgError> {
        let deadline = config
            .connect_timeout
            .map(|timeout| Instant::now() + timeout);
        let mut last = PgError::new("08001", "no Postgres host to connect to");
        for (index, host) in config.hosts.iter().enumerate() {
            match Self::connect_host(config, host, config.port(index), deadline) {
                Ok(connection) => return Ok(connection),
                // A server that refused the credentials or the database answers the same on every host.
                Err(error) if !error.sqlstate.starts_with("08") => return Err(error),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    fn connect_host(
        config: &Config,
        host: &str,
        port: u16,
        deadline: Option<Instant>,
    ) -> Result<Self, PgError> {
        let refused = |error: io::Error| {
            PgError::new(
                "08006",
                format!("connection to server at \"{host}\", port {port} failed: {error}"),
            )
        };
        let remaining = || {
            deadline.map(|deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_millis(1))
            })
        };
        #[cfg(unix)]
        let unix_socket = host.starts_with('/') || host.starts_with('@');
        #[cfg(not(unix))]
        let unix_socket = false;
        let mut stream = if unix_socket {
            #[cfg(unix)]
            {
                let path = format!("{host}/.s.PGSQL.{port}");
                let stream = std::os::unix::net::UnixStream::connect(&path).map_err(refused)?;
                Stream::Unix(stream)
            }
            #[cfg(not(unix))]
            unreachable!()
        } else {
            let addresses = (host, port).to_socket_addrs().map_err(refused)?;
            let mut connected = None;
            let mut failure = io::Error::new(io::ErrorKind::NotFound, "no address");
            for address in addresses {
                let attempt = match remaining() {
                    Some(timeout) => TcpStream::connect_timeout(&address, timeout),
                    None => TcpStream::connect(address),
                };
                match attempt {
                    Ok(stream) => {
                        connected = Some(stream);
                        break;
                    }
                    Err(error) => failure = error,
                }
            }
            let stream = connected.ok_or_else(|| refused(failure))?;
            stream.set_nodelay(true).map_err(refused)?;
            Stream::Tcp(stream)
        };
        stream.set_timeout(remaining()).map_err(refused)?;

        if !unix_socket && config.ssl_mode != SslMode::Disable && config.ssl_mode != SslMode::Allow
        {
            stream = negotiate_tls(stream, config, host)?;
        }

        let mut connection = Connection {
            stream,
            read: BytesMut::with_capacity(READ_CHUNK),
            write: BytesMut::with_capacity(4096),
            statements: rustc_hash::FxHashMap::default(),
            next_statement: 0,
            closing: Vec::new(),
            close_syncs: 0,
            status: b'I',
            pending_begin: false,
            broken: false,
            owner: std::process::id(),
            parameters: HashMap::new(),
        };
        connection.startup(config)?;
        connection
            .stream
            .set_timeout(None)
            .map_err(|error| PgError::io(&error))?;
        Ok(connection)
    }

    fn startup(&mut self, config: &Config) -> Result<(), PgError> {
        let mut parameters: Vec<(&str, &str)> =
            vec![("user", &config.user), ("client_encoding", "UTF8")];
        if let Some(dbname) = &config.dbname {
            parameters.push(("database", dbname));
        }
        if let Some(name) = &config.application_name {
            parameters.push(("application_name", name));
        }
        if let Some(options) = &config.options {
            parameters.push(("options", options));
        }
        self.write.clear();
        frontend::startup_message(parameters.iter().copied(), &mut self.write)
            .map_err(|error| PgError::io(&error))?;
        self.flush()?;
        let password = || {
            config
                .password
                .as_deref()
                .ok_or_else(|| PgError::new("08004", "fe_sendauth: no password supplied"))
        };
        let mut scram: Option<ScramSha256> = None;
        loop {
            match self.next_message()? {
                Message::AuthenticationOk => {}
                Message::AuthenticationCleartextPassword => {
                    self.write.clear();
                    frontend::password_message(password()?.as_bytes(), &mut self.write)
                        .map_err(|error| PgError::io(&error))?;
                    self.flush()?;
                }
                Message::AuthenticationMd5Password(body) => {
                    let hash = postgres_protocol::authentication::md5_hash(
                        config.user.as_bytes(),
                        password()?.as_bytes(),
                        body.salt(),
                    );
                    self.write.clear();
                    frontend::password_message(hash.as_bytes(), &mut self.write)
                        .map_err(|error| PgError::io(&error))?;
                    self.flush()?;
                }
                Message::AuthenticationSasl(body) => {
                    let mut offered = false;
                    let mut mechanisms = body.mechanisms();
                    while let Some(mechanism) =
                        mechanisms.next().map_err(|error| PgError::io(&error))?
                    {
                        offered |= mechanism == sasl::SCRAM_SHA_256;
                    }
                    if !offered {
                        return Err(PgError::new(
                            "08004",
                            "the server requires a SASL mechanism the pam-pg driver does not support",
                        ));
                    }
                    // No channel binding: the "n" flag, which the server accepts with or without TLS.
                    let state =
                        ScramSha256::new(password()?.as_bytes(), ChannelBinding::unsupported());
                    self.write.clear();
                    frontend::sasl_initial_response(
                        sasl::SCRAM_SHA_256,
                        state.message(),
                        &mut self.write,
                    )
                    .map_err(|error| PgError::io(&error))?;
                    self.flush()?;
                    scram = Some(state);
                }
                Message::AuthenticationSaslContinue(body) => {
                    let state = scram
                        .as_mut()
                        .ok_or_else(|| PgError::new("08P01", "unexpected SASL continuation"))?;
                    state.update(body.data()).map_err(|error| {
                        PgError::new("28000", format!("SCRAM authentication failed: {error}"))
                    })?;
                    self.write.clear();
                    frontend::sasl_response(state.message(), &mut self.write)
                        .map_err(|error| PgError::io(&error))?;
                    self.flush()?;
                }
                Message::AuthenticationSaslFinal(body) => {
                    let state = scram
                        .as_mut()
                        .ok_or_else(|| PgError::new("08P01", "unexpected SASL final message"))?;
                    state.finish(body.data()).map_err(|error| {
                        PgError::new("28000", format!("SCRAM server signature mismatch: {error}"))
                    })?;
                }
                Message::AuthenticationKerberosV5
                | Message::AuthenticationScmCredential
                | Message::AuthenticationGss
                | Message::AuthenticationSspi
                | Message::AuthenticationGssContinue(_) => {
                    return Err(PgError::new(
                        "08004",
                        "the server requires an authentication method the pam-pg driver does not support",
                    ));
                }
                Message::BackendKeyData(_) | Message::NoticeResponse(_) => {}
                Message::ParameterStatus(body) => self.parameter_status(&body),
                Message::ErrorResponse(body) => return Err(PgError::from_fields(body.fields())),
                Message::ReadyForQuery(body) => {
                    self.status = body.status();
                    return Ok(());
                }
                _ => return Err(PgError::new("08P01", "unexpected message during startup")),
            }
        }
    }

    fn parameter_status(
        &mut self,
        body: &postgres_protocol::message::backend::ParameterStatusBody,
    ) {
        if let (Ok(name), Ok(value)) = (body.name(), body.value()) {
            self.parameters.insert(name.to_owned(), value.to_owned());
        }
    }

    /// A server parameter reported at startup or since (`server_version`, `TimeZone`, ...).
    pub fn parameter(&self, name: &str) -> Option<&str> {
        self.parameters.get(name).map(String::as_str)
    }

    /// False once the session cannot be used: I/O failed mid-command or the
    /// process forked after opening it.
    pub fn usable(&self) -> bool {
        !self.broken && self.owner == std::process::id()
    }

    pub fn transaction_status(&self) -> TransactionStatus {
        if self.pending_begin {
            return TransactionStatus::InTransaction;
        }
        match self.status {
            b'T' => TransactionStatus::InTransaction,
            b'E' => TransactionStatus::Failed,
            _ => TransactionStatus::Idle,
        }
    }

    pub fn in_transaction(&self) -> bool {
        self.transaction_status() != TransactionStatus::Idle
    }

    pub fn begin(&mut self) -> Result<(), PgError> {
        self.check()?;
        if self.in_transaction() {
            return Err(PgError::new(
                "25001",
                "There is already an active transaction",
            ));
        }
        self.pending_begin = true;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<(), PgError> {
        self.finish_transaction("COMMIT")
    }

    pub fn rollback(&mut self) -> Result<(), PgError> {
        self.finish_transaction("ROLLBACK")
    }

    fn finish_transaction(&mut self, command: &str) -> Result<(), PgError> {
        self.check()?;
        if self.pending_begin {
            // Nothing ran inside it: BEGIN + COMMIT/ROLLBACK would change nothing.
            self.pending_begin = false;
            return Ok(());
        }
        if self.status == b'I' {
            return Err(PgError::new("25P01", "There is no active transaction"));
        }
        self.simple(command).map(|_| ())
    }

    /// Runs `sql` with the simple query protocol (several statements allowed,
    /// no parameters); returns the rows affected by the last one.
    pub fn simple(&mut self, sql: &str) -> Result<u64, PgError> {
        self.check()?;
        if sql.as_bytes().contains(&0) {
            return Err(PgError::new("22021", "the statement contains a NUL byte"));
        }
        if self.pending_begin {
            self.pending_begin = false;
            self.round_trip(|connection| {
                frontend::query("BEGIN", &mut connection.write).map_err(|error| PgError::io(&error))
            })?;
        }
        self.round_trip(|connection| {
            frontend::query(sql, &mut connection.write).map_err(|error| PgError::io(&error))
        })
        .map(|result| result.affected)
    }

    fn round_trip(
        &mut self,
        encode: impl FnOnce(&mut Self) -> Result<(), PgError>,
    ) -> Result<QueryResult, PgError> {
        self.write.clear();
        self.encode_closes()?;
        encode(self)?;
        self.flush()?;
        let mut result = QueryResult::empty();
        let error = self.read_until_ready(&mut result, None, false)?;
        self.trim_buffers();
        match error {
            Some(error) => Err(error),
            None => Ok(result),
        }
    }

    /// Closes the server-side statement of `sql` (sent with the next command).
    /// pdo_pgsql deallocates a statement with its PDOStatement; the driver calls
    /// this from the statement's destructor so a statement never outlives the
    /// object that prepared it (its parameter types were inferred from the
    /// schema of that moment).
    pub fn release(&mut self, sql: &str) {
        if let Some(statement) = self.statements.remove(sql) {
            self.closing.push(statement.name);
        }
    }

    /// Runs a prepared statement with `params` bound to its placeholders.
    pub fn execute(&mut self, sql: &str, params: &[Param<'_>]) -> Result<QueryResult, PgError> {
        self.check()?;
        match self.execute_once(sql, params) {
            // The statement went stale (DDL changed its result shape, or the
            // session lost it): outside a transaction it is prepared again.
            Err(error)
                if (error.sqlstate == "0A000" && error.message.contains("cached plan"))
                    || error.sqlstate == "26000" =>
            {
                if error.sqlstate == "26000" {
                    self.forget_statements(false);
                } else if let Some(statement) = self.statements.remove(sql) {
                    self.closing.push(statement.name);
                }
                if self.in_transaction() {
                    return Err(error);
                }
                self.execute_once(sql, params)
            }
            other => other,
        }
    }

    fn execute_once(&mut self, sql: &str, params: &[Param<'_>]) -> Result<QueryResult, PgError> {
        if sql.as_bytes().contains(&0) {
            return Err(PgError::new("22021", "the statement contains a NUL byte"));
        }
        // One lookup; the cached parts are reference counted, not copied.
        let cached = self.statements.get(sql).map(|statement| {
            (
                statement.name.clone(),
                statement.placeholders.clone(),
                statement.columns.clone(),
            )
        });
        let (name, placeholders, cached_columns, rewritten) = match cached {
            Some((name, placeholders, columns)) => (name, placeholders, Some(columns), None),
            None => {
                if self.statements.len() >= STATEMENT_CACHE_LIMIT {
                    self.forget_statements(true);
                }
                let (rewritten, placeholders) = rewrite_placeholders(sql)?;
                self.next_statement += 1;
                (
                    Arc::from(format!("pam{}", self.next_statement)),
                    Arc::new(placeholders),
                    None,
                    Some(rewritten),
                )
            }
        };
        let ordered = order_params(&placeholders, params)?;

        self.write.clear();
        self.encode_closes()?;
        let begin = std::mem::take(&mut self.pending_begin);
        if begin {
            // In the same pipeline: an error in BEGIN skips the statement too.
            encode_unnamed(&mut self.write, "BEGIN")?;
        }
        if let Some(rewritten) = &rewritten {
            frontend::parse(&name, rewritten, std::iter::empty(), &mut self.write)
                .map_err(|error| PgError::io(&error))?;
            frontend::describe(b'S', &name, &mut self.write)
                .map_err(|error| PgError::io(&error))?;
        }
        let formats = ordered
            .iter()
            .map(|value| i16::from(matches!(value, Value::Binary(_))));
        frontend::bind(
            "",
            &name,
            formats,
            ordered.iter(),
            |value, buffer| match value {
                Value::Null => Ok(IsNull::Yes),
                Value::Text(bytes) | Value::Binary(bytes) => {
                    buffer.put_slice(bytes);
                    Ok(IsNull::No)
                }
            },
            std::iter::empty(),
            &mut self.write,
        )
        .map_err(|_| {
            PgError::new(
                "54000",
                "too many parameters or a parameter too large for Postgres",
            )
        })?;
        frontend::execute("", 0, &mut self.write).map_err(|error| PgError::io(&error))?;
        frontend::sync(&mut self.write);
        self.flush()?;

        let mut result = QueryResult::empty();
        if let Some(columns) = cached_columns {
            result.columns = columns;
        }
        let mut described: Option<Vec<Column>> = None;
        let describe = if rewritten.is_some() {
            Some(&mut described)
        } else {
            None
        };
        let error = self.read_until_ready(&mut result, describe, begin)?;
        self.trim_buffers();
        if rewritten.is_some()
            && let Some(columns) = described
        {
            // Parsed on the server: cache it even when Bind or Execute failed.
            let columns: Arc<[Column]> = Arc::from(columns);
            if result.columns.is_empty() {
                result.columns = columns.clone();
            }
            self.statements.insert(
                Box::from(sql),
                Statement {
                    name,
                    placeholders,
                    columns,
                },
            );
        }
        match error {
            Some(mut error) => {
                // A Bind/placeholder count mismatch is the caller's error, not a broken connection.
                if error.sqlstate == "08P01" && error.message.contains("bind message supplies") {
                    error.sqlstate = "HY093".to_owned();
                }
                Err(error)
            }
            None => Ok(result),
        }
    }

    /// Reads responses until the command's `ReadyForQuery`; returns the first
    /// server error of the command. `describe` receives the statement's
    /// columns (from `Describe`); with `begin`, the first completion is the
    /// pipelined `BEGIN`'s. The `Sync` groups of pending closes come first and
    /// their outcome is not the command's.
    fn read_until_ready(
        &mut self,
        result: &mut QueryResult,
        mut describe: Option<&mut Option<Vec<Column>>>,
        mut begin: bool,
    ) -> Result<Option<PgError>, PgError> {
        let mut error: Option<PgError> = None;
        let mut awaiting_description = describe.is_some();
        loop {
            let message = self.next_message()?;
            match message {
                Message::DataRow(row) => {
                    if self.close_syncs == 0 {
                        result.rows.push(row);
                    }
                }
                Message::CommandComplete(body) => {
                    if self.close_syncs > 0 {
                        continue;
                    }
                    if std::mem::take(&mut begin) {
                        continue;
                    }
                    let tag = body.tag().unwrap_or("");
                    result.affected = tag
                        .rsplit(' ')
                        .next()
                        .and_then(|count| count.parse().ok())
                        .unwrap_or(0);
                }
                Message::NoData => {
                    if awaiting_description && self.close_syncs == 0 {
                        if let Some(slot) = describe.as_deref_mut() {
                            *slot = Some(Vec::new());
                        }
                        awaiting_description = false;
                    }
                }
                Message::ParseComplete
                | Message::BindComplete
                | Message::CloseComplete
                | Message::EmptyQueryResponse
                | Message::PortalSuspended
                | Message::ParameterDescription(_) => {}
                Message::RowDescription(body) => {
                    let mut columns = Vec::new();
                    let mut fields = body.fields();
                    while let Some(field) = fields
                        .next()
                        .map_err(|error| self.protocol_error(&error.to_string()))?
                    {
                        columns.push(Column {
                            name: field.name().to_owned(),
                            type_oid: field.type_oid(),
                        });
                    }
                    if awaiting_description {
                        awaiting_description = false;
                        if let Some(slot) = describe.as_deref_mut() {
                            *slot = Some(columns);
                        }
                    } else {
                        // Simple query: the description of the rows that follow.
                        result.columns = Arc::from(columns);
                        result.rows.clear();
                    }
                }
                Message::ErrorResponse(body) => {
                    begin = false;
                    if self.close_syncs == 0 && error.is_none() {
                        error = Some(PgError::from_fields(body.fields()));
                    }
                }
                Message::ReadyForQuery(body) => {
                    self.status = body.status();
                    if self.close_syncs > 0 {
                        self.close_syncs -= 1;
                        continue;
                    }
                    return Ok(error);
                }
                Message::CopyInResponse(_) => {
                    self.write.clear();
                    frontend::copy_fail(
                        "COPY FROM STDIN is not supported by the pam-pg driver",
                        &mut self.write,
                    )
                    .map_err(|error| PgError::io(&error))?;
                    self.flush()?;
                }
                Message::CopyOutResponse(_)
                | Message::CopyData(_)
                | Message::CopyDone
                | Message::NoticeResponse(_)
                | Message::NotificationResponse(_) => {}
                Message::ParameterStatus(body) => self.parameter_status(&body),
                _ => return Err(self.protocol_error("unexpected message")),
            }
        }
    }

    fn protocol_error(&mut self, what: &str) -> PgError {
        self.broken = true;
        PgError::new("08P01", format!("Postgres protocol error: {what}"))
    }

    fn check(&self) -> Result<(), PgError> {
        if self.broken {
            return Err(PgError::new(
                "08006",
                "The Postgres session was interrupted and must reconnect.",
            ));
        }
        if self.owner != std::process::id() {
            return Err(PgError::new(
                "08006",
                "The Postgres session belongs to the parent process and must reconnect.",
            ));
        }
        Ok(())
    }

    /// Closes every cached statement (sent before the next command).
    fn forget_statements(&mut self, close: bool) {
        for (_, statement) in self.statements.drain() {
            if close {
                self.closing.push(statement.name);
            }
        }
    }

    fn encode_closes(&mut self) -> Result<(), PgError> {
        if self.closing.is_empty() {
            return Ok(());
        }
        for name in self.closing.drain(..) {
            frontend::close(b'S', &name, &mut self.write).map_err(|error| PgError::io(&error))?;
        }
        // In their own Sync group: a failure here does not touch the next command.
        frontend::sync(&mut self.write);
        self.close_syncs += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<(), PgError> {
        if let Err(error) = self
            .stream
            .write_all(&self.write)
            .and_then(|()| self.stream.flush())
        {
            self.broken = true;
            return Err(PgError::io(&error));
        }
        Ok(())
    }

    fn next_message(&mut self) -> Result<Message, PgError> {
        loop {
            match Message::parse(&mut self.read) {
                Ok(Some(message)) => return Ok(message),
                Ok(None) => {}
                Err(error) => {
                    self.broken = true;
                    return Err(PgError::new(
                        "08P01",
                        format!("Postgres protocol error: {error}"),
                    ));
                }
            }
            self.read.reserve(READ_CHUNK);
            let spare = self.read.spare_capacity_mut();
            // SAFETY: read(2) only writes into the spare capacity and reports how much;
            // exactly that many bytes are marked initialized below.
            let buffer = unsafe {
                std::slice::from_raw_parts_mut(spare.as_mut_ptr().cast::<u8>(), spare.len())
            };
            match self.stream.read(buffer) {
                Ok(0) => {
                    self.broken = true;
                    return Err(PgError::new(
                        "08006",
                        "server closed the connection unexpectedly",
                    ));
                }
                Ok(read) => {
                    // SAFETY: `read` bytes were written by the read above.
                    unsafe { self.read.set_len(self.read.len() + read) };
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => {
                    self.broken = true;
                    return Err(PgError::io(&error));
                }
            }
        }
    }

    fn trim_buffers(&mut self) {
        if self.read.capacity() > READ_BUFFER_KEEP && self.read.is_empty() {
            self.read = BytesMut::with_capacity(READ_CHUNK);
        }
        if self.write.capacity() > READ_BUFFER_KEEP {
            self.write = BytesMut::with_capacity(4096);
        }
    }

    /// Statements prepared in this session.
    #[cfg(test)]
    pub fn prepared_statements(&self) -> usize {
        self.statements.len()
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.usable() {
            self.write.clear();
            frontend::terminate(&mut self.write);
            let _ = self.stream.set_timeout(Some(Duration::from_millis(100)));
            let _ = self.stream.write_all(&self.write);
        }
    }
}

fn encode_unnamed(buffer: &mut BytesMut, sql: &str) -> Result<(), PgError> {
    frontend::parse("", sql, std::iter::empty(), buffer).map_err(|error| PgError::io(&error))?;
    frontend::bind(
        "",
        "",
        std::iter::empty(),
        std::iter::empty::<Value<'_>>(),
        |_, _| Ok(IsNull::Yes),
        std::iter::empty(),
        buffer,
    )
    .map_err(|_| PgError::new("54000", "bind failed"))?;
    frontend::execute("", 0, buffer).map_err(|error| PgError::io(&error))
}

/// Values in `$n` order, checked against the placeholders like PDO does.
fn order_params<'a>(
    placeholders: &Placeholders,
    params: &[Param<'a>],
) -> Result<Vec<Value<'a>>, PgError> {
    let mismatch = || {
        PgError::new(
            "HY093",
            "Invalid parameter number: number of bound variables does not match number of tokens",
        )
    };
    match placeholders {
        Placeholders::Positional(count) => {
            if params.iter().any(|param| param.name.is_some()) {
                return Err(PgError::new(
                    "HY093",
                    "Invalid parameter number: parameter was not defined",
                ));
            }
            if params.len() != *count {
                return Err(mismatch());
            }
            Ok(params.iter().map(|param| param.value).collect())
        }
        Placeholders::Named(names) => {
            if params.len() != names.len() {
                return Err(mismatch());
            }
            names
                .iter()
                .map(|name| {
                    params
                        .iter()
                        .find(|param| param.name == Some(name.as_str()))
                        .map(|param| param.value)
                        .ok_or_else(|| {
                            PgError::new(
                                "HY093",
                                "Invalid parameter number: parameter was not defined",
                            )
                        })
                })
                .collect()
        }
    }
}

fn negotiate_tls(mut stream: Stream, config: &Config, host: &str) -> Result<Stream, PgError> {
    let mut request = BytesMut::with_capacity(8);
    frontend::ssl_request(&mut request);
    let mut answer = [0u8; 1];
    stream
        .write_all(&request)
        .and_then(|()| stream.read_exact(&mut answer))
        .map_err(|error| PgError::io(&error))?;
    let Stream::Tcp(tcp) = stream else {
        return Ok(stream);
    };
    if answer[0] != b'S' {
        if matches!(
            config.ssl_mode,
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull
        ) {
            return Err(PgError::new(
                "08001",
                "server does not support SSL, but SSL was required",
            ));
        }
        return Ok(Stream::Tcp(tcp));
    }
    let tls = tls::client_config(config)?;
    let name = rustls_pki_types::ServerName::try_from(host.to_owned())
        .map_err(|_| PgError::new("08001", format!("invalid TLS server name \"{host}\"")))?;
    let mut session = rustls::ClientConnection::new(tls, name)
        .map_err(|error| PgError::new("08001", format!("TLS setup failed: {error}")))?;
    let mut tcp = tcp;
    while session.is_handshaking() {
        session
            .complete_io(&mut tcp)
            .map_err(|error| PgError::new("08001", format!("SSL handshake failed: {error}")))?;
    }
    Ok(Stream::Tls(Box::new(rustls::StreamOwned::new(
        session, tcp,
    ))))
}

mod tls {
    use std::sync::Arc;

    use rustls::client::WebPkiServerVerifier;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::CryptoProvider;
    use rustls::{DigitallySignedStruct, Error, RootCertStore, SignatureScheme};
    use rustls_pki_types::pem::PemObject;
    use rustls_pki_types::{CertificateDer, ServerName, UnixTime};

    use super::{Config, PgError, SslMode};

    pub fn client_config(config: &Config) -> Result<Arc<rustls::ClientConfig>, PgError> {
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let builder = rustls::ClientConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .map_err(|error| PgError::new("08001", format!("TLS setup failed: {error}")))?;
        let verifier: Arc<dyn ServerCertVerifier> = match config.ssl_mode {
            SslMode::VerifyCa | SslMode::VerifyFull => {
                let roots = Arc::new(roots(config)?);
                let webpki = WebPkiServerVerifier::builder_with_provider(roots, provider.clone())
                    .build()
                    .map_err(|error| PgError::new("08001", format!("TLS setup failed: {error}")))?;
                if config.ssl_mode == SslMode::VerifyCa {
                    Arc::new(ChainOnly(webpki))
                } else {
                    webpki
                }
            }
            // prefer/require: encrypted, the certificate is not checked (libpq's
            // behavior without a root certificate); the handshake signatures still are.
            _ => Arc::new(EncryptOnly(provider)),
        };
        Ok(Arc::new(
            builder
                .dangerous()
                .with_custom_certificate_verifier(verifier)
                .with_no_client_auth(),
        ))
    }

    fn roots(config: &Config) -> Result<RootCertStore, PgError> {
        let mut store = RootCertStore::empty();
        match config.ssl_root_cert.as_deref() {
            None | Some("system") => store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
            Some(path) => {
                let certificates = CertificateDer::pem_file_iter(path).map_err(|error| {
                    PgError::new(
                        "08001",
                        format!("could not read root certificate file \"{path}\": {error}"),
                    )
                })?;
                for certificate in certificates {
                    let certificate = certificate.map_err(|error| {
                        PgError::new(
                            "08001",
                            format!("invalid root certificate in \"{path}\": {error}"),
                        )
                    })?;
                    store.add(certificate).map_err(|error| {
                        PgError::new(
                            "08001",
                            format!("invalid root certificate in \"{path}\": {error}"),
                        )
                    })?;
                }
            }
        }
        Ok(store)
    }

    #[derive(Debug)]
    struct EncryptOnly(Arc<CryptoProvider>);

    impl ServerCertVerifier for EncryptOnly {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }

    /// verify-ca: the chain must lead to a trusted root; the host name is not checked.
    #[derive(Debug)]
    struct ChainOnly(Arc<WebPkiServerVerifier>);

    impl ServerCertVerifier for ChainOnly {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            intermediates: &[CertificateDer<'_>],
            server_name: &ServerName<'_>,
            ocsp_response: &[u8],
            now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            match self.0.verify_server_cert(
                end_entity,
                intermediates,
                server_name,
                ocsp_response,
                now,
            ) {
                Err(Error::InvalidCertificate(rustls::CertificateError::NotValidForName))
                | Err(Error::InvalidCertificate(
                    rustls::CertificateError::NotValidForNameContext { .. },
                )) => Ok(ServerCertVerified::assertion()),
                other => other,
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            self.0.verify_tls12_signature(message, cert, dss)
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            self.0.verify_tls13_signature(message, cert, dss)
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.0.supported_verify_schemes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(sql: &str) -> (String, Placeholders) {
        rewrite_placeholders(sql).expect("rewrite")
    }

    #[test]
    fn rewrites_positional_placeholders_outside_literals_and_comments() {
        assert_eq!(
            rewrite("SELECT * FROM t WHERE a = ? AND b = ?"),
            (
                "SELECT * FROM t WHERE a = $1 AND b = $2".to_owned(),
                Placeholders::Positional(2)
            )
        );
        assert_eq!(
            rewrite("SELECT '?', \"a?\", ? -- ?\n, /* ? /* ? */ ? */ ?"),
            (
                "SELECT '?', \"a?\", $1 -- ?\n, /* ? /* ? */ ? */ $2".to_owned(),
                Placeholders::Positional(2)
            )
        );
        assert_eq!(
            rewrite("SELECT 'it''s ?', E'a\\' ?', ?"),
            (
                "SELECT 'it''s ?', E'a\\' ?', $1".to_owned(),
                Placeholders::Positional(1)
            )
        );
        assert_eq!(
            rewrite("SELECT $$ ? $$, $tag$ ?$x$ $tag$, ?"),
            (
                "SELECT $$ ? $$, $tag$ ?$x$ $tag$, $1".to_owned(),
                Placeholders::Positional(1)
            )
        );
        assert_eq!(
            rewrite("SELECT 1"),
            ("SELECT 1".to_owned(), Placeholders::Positional(0))
        );
    }

    #[test]
    fn escaped_question_marks_and_casts_are_not_placeholders() {
        assert_eq!(
            rewrite("SELECT data ?? 'k', id::text FROM t WHERE id = ?"),
            (
                "SELECT data ? 'k', id::text FROM t WHERE id = $1".to_owned(),
                Placeholders::Positional(1)
            )
        );
    }

    #[test]
    fn named_placeholders_share_a_number_per_name() {
        assert_eq!(
            rewrite("SELECT :a, :b_2, :a, x::int, ':c'"),
            (
                "SELECT $1, $2, $1, x::int, ':c'".to_owned(),
                Placeholders::Named(vec!["a".to_owned(), "b_2".to_owned()])
            )
        );
        assert_eq!(
            rewrite_placeholders("SELECT ?, :a").unwrap_err().sqlstate,
            "HY093"
        );
        assert_eq!(
            rewrite_placeholders("SELECT :a, ?").unwrap_err().sqlstate,
            "HY093"
        );
    }

    #[test]
    fn values_are_checked_against_the_placeholders() {
        let text = |value: &'static str| Param {
            name: None,
            value: Value::Text(value.as_bytes()),
        };
        assert!(order_params(&Placeholders::Positional(2), &[text("1")]).is_err());
        assert_eq!(
            order_params(&Placeholders::Positional(1), &[text("1")])
                .unwrap()
                .len(),
            1
        );
        let named = Placeholders::Named(vec!["b".to_owned(), "a".to_owned()]);
        let params = [
            Param {
                name: Some("a"),
                value: Value::Text(b"A"),
            },
            Param {
                name: Some("b"),
                value: Value::Null,
            },
        ];
        let ordered = order_params(&named, &params).unwrap();
        assert!(matches!(ordered[0], Value::Null));
        assert!(matches!(ordered[1], Value::Text(b"A")));
        let wrong = [
            Param {
                name: Some("a"),
                value: Value::Null,
            },
            Param {
                name: Some("c"),
                value: Value::Null,
            },
        ];
        assert_eq!(order_params(&named, &wrong).unwrap_err().sqlstate, "HY093");
    }

    #[test]
    fn parses_libpq_keywords() {
        let config = Config::from_pairs([
            ("host", "/tmp/pg,db.internal"),
            ("port", "5433,5434"),
            ("dbname", "app"),
            ("user", "u"),
            ("password", "p"),
            ("sslmode", "disable"),
            ("connect_timeout", "1"),
        ])
        .unwrap();
        assert_eq!(config.hosts, vec!["/tmp/pg", "db.internal"]);
        assert_eq!(config.port(1), 5434);
        assert_eq!(config.ssl_mode, SslMode::Disable);
        assert_eq!(config.connect_timeout, Some(Duration::from_secs(2)));
        assert!(Config::from_pairs([("sslcert", "x")]).is_err());
        assert!(Config::from_pairs([("sslmode", "sometimes")]).is_err());
    }

    /// Against a live server: `PAM_PG_TEST="host=127.0.0.1 port=54329 user=bench password=bench dbname=bench"`.
    #[test]
    fn live_round_trips() {
        let Ok(conninfo) = std::env::var("PAM_PG_TEST") else {
            return;
        };
        let pairs: Vec<(String, String)> = conninfo
            .split_whitespace()
            .filter_map(|pair| {
                pair.split_once('=')
                    .map(|(k, v)| (k.to_owned(), v.to_owned()))
            })
            .collect();
        let config =
            Config::from_pairs(pairs.iter().map(|(k, v)| (k.as_str(), v.as_str()))).unwrap();
        let mut connection = Connection::connect(&config).unwrap();
        assert!(connection.parameter("server_version").is_some());

        let text = |value: &'static str| Param {
            name: None,
            value: Value::Text(value.as_bytes()),
        };
        let result = connection
            .execute("SELECT ?::int + 1 AS n, NULL::text AS t", &[text("41")])
            .unwrap();
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.columns[0].name, "n");
        assert_eq!(result.rows.len(), 1);
        let row = &result.rows[0];
        let cells: Vec<Option<&[u8]>> = row
            .ranges()
            .map(|range| Ok(range.map(|range| &row.buffer()[range])))
            .collect()
            .unwrap();
        assert_eq!(cells, vec![Some(&b"42"[..]), None]);
        // Cached: the second run binds by name.
        connection
            .execute("SELECT ?::int + 1 AS n, NULL::text AS t", &[text("1")])
            .unwrap();
        assert_eq!(connection.prepared_statements(), 1);

        // An injection attempt is only ever a value.
        let evil = connection
            .execute("SELECT ?::text AS v", &[text("'; DROP TABLE x; --")])
            .unwrap();
        assert_eq!(evil.rows.len(), 1);

        // Errors keep the session usable and carry the SQLSTATE.
        let error = connection
            .execute("SELECT * FROM missing_table_pam", &[])
            .unwrap_err();
        assert_eq!(error.sqlstate, "42P01");
        assert!(connection.usable());
        assert_eq!(
            connection.execute("SELECT ?", &[]).unwrap_err().sqlstate,
            "HY093"
        );

        // Lazy BEGIN travels with the first statement; a failure aborts the transaction.
        connection
            .simple("CREATE TEMP TABLE pam_pg_t (id int PRIMARY KEY)")
            .unwrap();
        connection.begin().unwrap();
        assert!(connection.in_transaction());
        connection
            .execute("INSERT INTO pam_pg_t VALUES (?)", &[text("1")])
            .unwrap();
        assert_eq!(
            connection.transaction_status(),
            TransactionStatus::InTransaction
        );
        let duplicate = connection
            .execute("INSERT INTO pam_pg_t VALUES (?)", &[text("1")])
            .unwrap_err();
        assert_eq!(duplicate.sqlstate, "23505");
        assert_eq!(connection.transaction_status(), TransactionStatus::Failed);
        connection.rollback().unwrap();
        assert!(!connection.in_transaction());
        let count = connection
            .execute("SELECT count(*) FROM pam_pg_t", &[])
            .unwrap();
        assert_eq!(&count.rows[0].buffer()[4..], b"0");
        connection.begin().unwrap();
        connection.commit().unwrap();
        assert_eq!(
            connection
                .simple("INSERT INTO pam_pg_t VALUES (1), (2)")
                .unwrap(),
            2
        );

        // DDL that changes a cached statement's result shape: prepared again.
        connection.execute("SELECT * FROM pam_pg_t", &[]).unwrap();
        connection
            .simple("ALTER TABLE pam_pg_t ADD COLUMN name text")
            .unwrap();
        let reshaped = connection.execute("SELECT * FROM pam_pg_t", &[]).unwrap();
        assert_eq!(reshaped.columns.len(), 2);
    }
}
