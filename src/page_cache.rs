//! Edge page cache: full responses served from the Rust/Tokio layer without
//! entering PHP, Varnish-style.
//!
//! Applications opt in per response with standard headers
//! (`Cache-Control: public, s-maxage=N, stale-while-revalidate=M`, `Vary`)
//! plus `Pam-Cache-Tags` for purges; the rules live in [`policy`]. The store
//! is bounded by bytes and entries (LRU), keeps one identity body per variant
//! with optional precompressed gzip/brotli copies, answers `If-None-Match`
//! with 304, serves stale entries while one request revalidates in the
//! background, and collapses concurrent misses into a single PHP execution.
//!
//! Each worker process owns its store. Purges reach sibling workers (and
//! `pam cache:purge`, queue workers, Artisan) through an append-only
//! invalidation log every worker tails.

pub mod policy;

use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes, HttpBody};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::Response;
use tokio::sync::{Semaphore, watch};

use policy::{Bypass, Encoding, Purge, RequestKey, RequestRules, ResponseRules, StorePolicy};

/// Most variants (distinct `Vary` values) kept per URL.
const MAX_VARIANTS: usize = 16;
/// Fixed per-entry overhead charged to the byte budget (maps, Arc, headers).
const ENTRY_OVERHEAD_BYTES: usize = 512;
/// How long an uncacheable answer stops misses for that URL from waiting on
/// one another (Varnish's hit-for-miss).
const UNCACHEABLE_MEMORY: Duration = Duration::from_secs(10);
/// The invalidation log is rotated past this size; readers purge everything
/// on rotation, so a lost line can never keep a stale page.
const INVALIDATION_LOG_MAX_BYTES: u64 = 1024 * 1024;
/// Workers poll the invalidation log at most this often.
const INVALIDATION_POLL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug)]
pub struct PageCacheConfig {
    pub requests: RequestRules,
    pub responses: ResponseRules,
    pub max_entries: usize,
    pub max_bytes: usize,
    pub max_object_bytes: usize,
    pub compress: bool,
    pub tag_headers: Vec<HeaderName>,
    pub invalidation_log: Option<PathBuf>,
    /// Adds `X-Pam-Cache: HIT|STALE|MISS|BYPASS` to responses.
    pub status_header: bool,
}

impl PageCacheConfig {
    pub fn enabled(&self) -> bool {
        (self.requests.header_driven || !self.requests.paths.is_empty())
            && self.max_entries > 0
            && self.max_bytes > 0
    }
}

pub struct Entry {
    id: u64,
    primary: String,
    host: String,
    path: String,
    query: String,
    vary: Vec<(HeaderName, Option<Vec<u8>>)>,
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
    /// Validators of the identity, brotli and gzip representations.
    etags: [String; 3],
    etag_headers: [HeaderValue; 3],
    encoded: OnceLock<Encoded>,
    stored_at: Instant,
    fresh_until: Instant,
    stale_until: Instant,
    tags: Box<[String]>,
    size: usize,
    last_access: AtomicU64,
    /// The tick this entry is filed under in the LRU order (write lock held).
    ordered_at: AtomicU64,
}

#[derive(Default)]
struct Encoded {
    gzip: Option<Bytes>,
    brotli: Option<Bytes>,
}

impl Entry {
    fn matches(&self, request: &HeaderMap) -> bool {
        self.vary
            .iter()
            .all(|(name, stored)| policy::vary_value_matches(request, name, stored.as_deref()))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HitKind {
    Fresh,
    Stale,
}

pub enum Lookup {
    Hit(Arc<Entry>, HitKind),
    Miss,
}

/// The single request allowed to fetch a URL from PHP; others wait on it.
pub struct FlightGuard {
    cache: Arc<PageCache>,
    primary: String,
    _done: watch::Sender<()>,
}

impl Drop for FlightGuard {
    fn drop(&mut self) {
        lock(&self.cache.flights).remove(&self.primary);
    }
}

pub enum Flight {
    Leader(FlightGuard),
    Follower(watch::Receiver<()>),
}

#[derive(Default)]
struct Inner {
    primaries: HashMap<String, Vec<u64>>,
    entries: HashMap<u64, Arc<Entry>>,
    order: BTreeMap<u64, u64>,
    bytes: usize,
    uncacheable: HashMap<String, Instant>,
}

#[derive(Default)]
pub struct Counters {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub stale: AtomicU64,
    pub collapsed: AtomicU64,
    pub purges: AtomicU64,
    pub purged_entries: AtomicU64,
    pub stores: AtomicU64,
    pub evictions: AtomicU64,
    pub not_modified: AtomicU64,
    pub revalidations: AtomicU64,
    pub bypasses: AtomicU64,
    pub uncacheable: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub hits: u64,
    pub misses: u64,
    pub stale: u64,
    pub collapsed: u64,
    pub purges: u64,
    pub purged_entries: u64,
    pub stores: u64,
    pub evictions: u64,
    pub not_modified: u64,
    pub revalidations: u64,
    pub bypasses: u64,
    pub uncacheable: u64,
    pub entries: u64,
    pub bytes: u64,
}

struct LogCursor {
    offset: u64,
    identity: Option<(u64, u64)>,
}

pub struct PageCache {
    config: PageCacheConfig,
    inner: RwLock<Inner>,
    flights: Mutex<HashMap<String, watch::Receiver<()>>>,
    clock: AtomicU64,
    next_id: AtomicU64,
    epoch: AtomicU64,
    compression: Arc<Semaphore>,
    log_cursor: Mutex<LogCursor>,
    next_log_poll: AtomicU64,
    started: Instant,
    pub counters: Counters,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl PageCache {
    pub fn new(config: PageCacheConfig) -> Arc<Self> {
        Arc::new(Self {
            config,
            inner: RwLock::new(Inner::default()),
            flights: Mutex::new(HashMap::new()),
            clock: AtomicU64::new(1),
            next_id: AtomicU64::new(1),
            epoch: AtomicU64::new(0),
            compression: Arc::new(Semaphore::new(1)),
            log_cursor: Mutex::new(LogCursor {
                offset: 0,
                identity: None,
            }),
            next_log_poll: AtomicU64::new(0),
            started: Instant::now(),
            counters: Counters::default(),
        })
    }

    pub fn config(&self) -> &PageCacheConfig {
        &self.config
    }

    pub fn enabled(&self) -> bool {
        self.config.enabled()
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Bumped by every purge; a fetch that started before a purge is not
    /// stored, so a purge can never be undone by a response rendered earlier.
    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn request_key(
        &self,
        method: &Method,
        scheme: &str,
        authority: Option<&str>,
        path: &str,
        query: Option<&str>,
        headers: &HeaderMap,
    ) -> Result<RequestKey, Bypass> {
        if !self.enabled() {
            return Err(Bypass::Disabled);
        }
        let key = policy::request_key(
            &self.config.requests,
            method,
            scheme,
            authority,
            path,
            query,
            headers,
        );
        if key.is_err() {
            self.counters.bypasses.fetch_add(1, Ordering::Relaxed);
        }
        key
    }

    pub fn lookup(&self, key: &RequestKey, request: &HeaderMap) -> Lookup {
        let now = Instant::now();
        let mut expired = false;
        let found = {
            let inner = self.read();
            inner.primaries.get(&key.primary).and_then(|ids| {
                ids.iter()
                    .filter_map(|id| inner.entries.get(id))
                    .find(|entry| entry.matches(request))
                    .and_then(|entry| {
                        if entry.fresh_until > now {
                            Some((entry.clone(), HitKind::Fresh))
                        } else if entry.stale_until > now {
                            Some((entry.clone(), HitKind::Stale))
                        } else {
                            expired = true;
                            None
                        }
                    })
            })
        };
        if expired {
            self.remove_expired(&key.primary, now);
        }
        match found {
            Some((entry, kind)) => {
                entry.last_access.store(
                    self.clock.fetch_add(1, Ordering::Relaxed),
                    Ordering::Relaxed,
                );
                Lookup::Hit(entry, kind)
            }
            None => Lookup::Miss,
        }
    }

    fn remove_expired(&self, primary: &str, now: Instant) {
        let mut inner = self.write();
        let ids: Vec<u64> = inner
            .primaries
            .get(primary)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| {
                        inner
                            .entries
                            .get(id)
                            .is_some_and(|entry| entry.stale_until <= now)
                    })
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            inner.remove(id);
        }
    }

    /// Claims the right to fetch `primary` from PHP.
    pub fn begin_flight(self: &Arc<Self>, primary: &str) -> Flight {
        let mut flights = lock(&self.flights);
        if let Some(receiver) = flights.get(primary) {
            return Flight::Follower(receiver.clone());
        }
        let (sender, receiver) = watch::channel(());
        flights.insert(primary.to_owned(), receiver);
        Flight::Leader(FlightGuard {
            cache: self.clone(),
            primary: primary.to_owned(),
            _done: sender,
        })
    }

    /// Whether `primary` answered uncacheable recently: its misses then go to
    /// PHP concurrently instead of queueing behind one another.
    pub fn recently_uncacheable(&self, primary: &str) -> bool {
        self.read()
            .uncacheable
            .get(primary)
            .is_some_and(|until| *until > Instant::now())
    }

    /// Removes the cache tag headers from a response and returns its tags.
    pub fn take_tags(&self, headers: &mut HeaderMap) -> Vec<String> {
        let mut tags = Vec::new();
        for name in &self.config.tag_headers {
            if headers.contains_key(name) {
                for tag in policy::parse_tags(headers.get_all(name).iter()) {
                    if tags.len() < policy::MAX_TAGS && !tags.contains(&tag) {
                        tags.push(tag);
                    }
                }
                headers.remove(name);
            }
        }
        tags
    }

    pub fn store_policy(
        &self,
        key: &RequestKey,
        status: StatusCode,
        headers: &HeaderMap,
    ) -> Result<StorePolicy, policy::NotStored> {
        policy::store_policy(&self.config.responses, key.forced_path, status, headers)
    }

    /// Records that `key` answered with an uncacheable response: matching
    /// stored variants are dropped (the page is no longer public) and misses
    /// stop collapsing for a while.
    pub fn reject(&self, key: &RequestKey, request: &HeaderMap) {
        self.counters.uncacheable.fetch_add(1, Ordering::Relaxed);
        let now = Instant::now();
        let mut inner = self.write();
        let ids: Vec<u64> = inner
            .primaries
            .get(&key.primary)
            .map(|ids| {
                ids.iter()
                    .copied()
                    .filter(|id| {
                        inner
                            .entries
                            .get(id)
                            .is_some_and(|entry| entry.matches(request))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for id in ids {
            inner.remove(id);
        }
        if inner.uncacheable.len() >= self.config.max_entries {
            inner.uncacheable.retain(|_, until| *until > now);
            if inner.uncacheable.len() >= self.config.max_entries {
                inner.uncacheable.clear();
            }
        }
        inner
            .uncacheable
            .insert(key.primary.clone(), now + UNCACHEABLE_MEMORY);
    }

    /// Stores a response. Returns `None` when it does not fit, the response
    /// varies on more headers than allowed, or a purge ran since `epoch`.
    #[allow(clippy::too_many_arguments)]
    pub fn store(
        self: &Arc<Self>,
        key: &RequestKey,
        request: &HeaderMap,
        status: StatusCode,
        response_headers: &HeaderMap,
        body: Bytes,
        store_policy: StorePolicy,
        tags: Vec<String>,
        epoch: u64,
    ) -> Option<Arc<Entry>> {
        if body.len() > self.config.max_object_bytes {
            return None;
        }
        let negotiates = !response_headers.contains_key(http::header::CONTENT_ENCODING);
        let vary_names = policy::vary_names(response_headers, negotiates)?;
        let vary_values = policy::vary_values(request, &vary_names);
        let mut headers = HeaderMap::with_capacity(response_headers.len() + 1);
        for (name, value) in response_headers {
            if policy::stored_header(name) {
                headers.append(name.clone(), value.clone());
            }
        }
        let etag = match headers
            .get(http::header::ETAG)
            .and_then(|value| value.to_str().ok())
        {
            Some(etag) => etag.to_owned(),
            None => {
                let etag = policy::etag_for(&body);
                headers.insert(
                    http::header::ETAG,
                    HeaderValue::from_str(&etag).expect("generated ETag is a valid header"),
                );
                etag
            }
        };
        let header_bytes: usize = headers
            .iter()
            .map(|(name, value)| name.as_str().len() + value.len() + 32)
            .sum();
        let size = ENTRY_OVERHEAD_BYTES
            + body.len()
            + header_bytes
            + 2 * key.primary.len()
            + key.host.len()
            + key.path.len()
            + key.query.len()
            + tags.iter().map(|tag| tag.len() + 24).sum::<usize>()
            + vary_values
                .iter()
                .map(|value| value.as_ref().map_or(0, Vec::len) + 32)
                .sum::<usize>();
        if size > self.config.max_bytes {
            return None;
        }
        let etags = [
            etag.clone(),
            policy::encoded_etag(&etag, "br"),
            policy::encoded_etag(&etag, "gzip"),
        ];
        let etag_headers = [
            HeaderValue::from_str(&etags[0]).ok()?,
            HeaderValue::from_str(&etags[1]).ok()?,
            HeaderValue::from_str(&etags[2]).ok()?,
        ];
        let now = Instant::now();
        let compress =
            self.config.compress && negotiates && policy::compressible(&headers, body.len());
        let entry = Arc::new(Entry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
            primary: key.primary.clone(),
            host: key.host.clone(),
            path: key.path.clone(),
            query: key.query.clone(),
            vary: vary_names.into_iter().zip(vary_values).collect(),
            status,
            headers,
            body,
            etags,
            etag_headers,
            encoded: OnceLock::new(),
            stored_at: now,
            fresh_until: now + store_policy.ttl,
            stale_until: now + store_policy.ttl + store_policy.stale_while_revalidate,
            tags: tags.into_boxed_slice(),
            size,
            last_access: AtomicU64::new(0),
            ordered_at: AtomicU64::new(0),
        });
        let tick = self.clock.fetch_add(1, Ordering::Relaxed);
        entry.last_access.store(tick, Ordering::Relaxed);
        entry.ordered_at.store(tick, Ordering::Relaxed);
        {
            let mut inner = self.write();
            // Checked under the write lock: purges take it too, so a purge
            // either sees this entry or bumped the epoch before this check.
            if self.epoch() != epoch {
                return None;
            }
            inner.uncacheable.remove(&key.primary);
            let request_matches: Vec<u64> = inner
                .primaries
                .get(&key.primary)
                .map(|ids| {
                    ids.iter()
                        .copied()
                        .filter(|id| {
                            inner
                                .entries
                                .get(id)
                                .is_some_and(|existing| existing.matches(request))
                        })
                        .collect()
                })
                .unwrap_or_default();
            for id in request_matches {
                inner.remove(id);
            }
            if inner
                .primaries
                .get(&key.primary)
                .is_some_and(|ids| ids.len() >= MAX_VARIANTS)
            {
                let oldest = inner.primaries[&key.primary]
                    .iter()
                    .copied()
                    .min_by_key(|id| inner.entries[id].last_access.load(Ordering::Relaxed));
                if let Some(oldest) = oldest {
                    inner.remove(oldest);
                }
            }
            self.evict_for(&mut inner, entry.size, 1);
            inner.insert(entry.clone());
        }
        self.counters.stores.fetch_add(1, Ordering::Relaxed);
        if compress {
            self.schedule_compression(entry.clone());
        }
        Some(entry)
    }

    /// Evicts least recently used entries until `bytes` and `entries` more fit.
    fn evict_for(&self, inner: &mut Inner, bytes: usize, entries: usize) {
        while !inner.order.is_empty()
            && (inner.bytes + bytes > self.config.max_bytes
                || inner.entries.len() + entries > self.config.max_entries)
        {
            let Some((tick, id)) = inner.order.pop_first() else {
                break;
            };
            let Some(entry) = inner.entries.get(&id) else {
                continue;
            };
            let accessed = entry.last_access.load(Ordering::Relaxed);
            if accessed > tick {
                // Touched since it was ordered: give it its real position.
                entry.ordered_at.store(accessed, Ordering::Relaxed);
                inner.order.insert(accessed, id);
                continue;
            }
            inner.remove(id);
            self.counters.evictions.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn schedule_compression(self: &Arc<Self>, entry: Arc<Entry>) {
        // One compression at a time per worker; when busy the entry is simply
        // served uncompressed, so a cache-busting load cannot pile up CPU work.
        let Ok(permit) = self.compression.clone().try_acquire_owned() else {
            return;
        };
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let cache = self.clone();
        runtime.spawn_blocking(move || {
            let _permit = permit;
            let encoded = compress(&entry.body);
            let added = encoded.gzip.as_ref().map_or(0, Bytes::len)
                + encoded.brotli.as_ref().map_or(0, Bytes::len);
            let mut inner = cache.write();
            if !inner
                .entries
                .get(&entry.id)
                .is_some_and(|stored| Arc::ptr_eq(stored, &entry))
            {
                return;
            }
            if entry.size + added > cache.config.max_bytes {
                return;
            }
            cache.evict_for(&mut inner, added, 0);
            if inner
                .entries
                .get(&entry.id)
                .is_some_and(|stored| Arc::ptr_eq(stored, &entry))
                && entry.encoded.set(encoded).is_ok()
            {
                inner.bytes += added;
            }
        });
    }

    /// Builds the response for a stored entry, honouring `If-None-Match`,
    /// `If-Modified-Since`, `Accept-Encoding` and `HEAD`.
    pub fn respond(
        &self,
        entry: &Entry,
        label: &'static str,
        request: &HeaderMap,
        head: bool,
    ) -> (Response, usize) {
        let encoded = entry.encoded.get();
        let encoding = policy::negotiate_encoding(
            request,
            encoded.is_some_and(|encoded| encoded.gzip.is_some()),
            encoded.is_some_and(|encoded| encoded.brotli.is_some()),
        );
        let (body, etag) = match (encoding, encoded) {
            (
                Encoding::Brotli,
                Some(Encoded {
                    brotli: Some(body), ..
                }),
            ) => (body.clone(), &entry.etag_headers[1]),
            (
                Encoding::Gzip,
                Some(Encoded {
                    gzip: Some(body), ..
                }),
            ) => (body.clone(), &entry.etag_headers[2]),
            _ => (entry.body.clone(), &entry.etag_headers[0]),
        };
        let mut headers = entry.headers.clone();
        headers.insert(http::header::ETAG, etag.clone());
        if encoding != Encoding::Identity {
            headers.insert(
                http::header::CONTENT_ENCODING,
                HeaderValue::from_static(encoding.token()),
            );
        }
        if encoded.is_some() && !vary_lists(&headers, "accept-encoding") {
            headers.append(
                http::header::VARY,
                HeaderValue::from_static("Accept-Encoding"),
            );
        }
        let age = entry.stored_at.elapsed().as_secs();
        headers.insert(http::header::AGE, HeaderValue::from(age));
        if self.config.status_header {
            headers.insert(
                HeaderName::from_static("x-pam-cache"),
                HeaderValue::from_static(label),
            );
        }
        let validators = [
            entry.etags[0].as_str(),
            entry.etags[1].as_str(),
            entry.etags[2].as_str(),
        ];
        let success = entry.status.is_success();
        if success
            && (policy::if_none_match_matches(request, &validators)
                || policy::not_modified_since(request, &entry.headers))
        {
            self.counters.not_modified.fetch_add(1, Ordering::Relaxed);
            let mut not_modified = HeaderMap::new();
            for name in [
                http::header::CACHE_CONTROL,
                http::header::CONTENT_LOCATION,
                http::header::ETAG,
                http::header::EXPIRES,
                http::header::LAST_MODIFIED,
                http::header::VARY,
                http::header::AGE,
                HeaderName::from_static("x-pam-cache"),
            ] {
                for value in headers.get_all(&name) {
                    not_modified.append(name.clone(), value.clone());
                }
            }
            let mut response = Response::new(Body::empty());
            *response.status_mut() = StatusCode::NOT_MODIFIED;
            *response.headers_mut() = not_modified;
            return (response, 0);
        }
        let length = body.len();
        headers.insert(http::header::CONTENT_LENGTH, HeaderValue::from(length));
        let mut response = if head {
            Response::new(Body::empty())
        } else {
            Response::new(Body::from(body))
        };
        *response.status_mut() = entry.status;
        *response.headers_mut() = headers;
        (response, if head { 0 } else { length })
    }

    /// Applies purges to this worker's store. Returns how many entries went.
    pub fn purge(&self, purges: &[Purge]) -> usize {
        if purges.is_empty() {
            return 0;
        }
        let mut inner = self.write();
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let before = inner.entries.len();
        if purges.contains(&Purge::All) {
            *inner = Inner::default();
        } else {
            let doomed: Vec<u64> = inner
                .entries
                .values()
                .filter(|entry| {
                    purges.iter().any(|purge| match purge {
                        Purge::All => true,
                        Purge::Tag(tag) => entry.tags.iter().any(|stored| stored == tag),
                        Purge::Url(pattern) => {
                            pattern.matches(&entry.host, &entry.path, &entry.query)
                        }
                    })
                })
                .map(|entry| entry.id)
                .collect();
            for id in doomed {
                inner.remove(id);
            }
        }
        let purged = before - inner.entries.len();
        self.counters
            .purges
            .fetch_add(purges.len() as u64, Ordering::Relaxed);
        self.counters
            .purged_entries
            .fetch_add(purged as u64, Ordering::Relaxed);
        purged
    }

    /// Purges locally and publishes to sibling workers through the log.
    pub fn purge_everywhere(&self, purges: &[Purge]) -> Result<usize, String> {
        let purged = self.purge(purges);
        if let Some(path) = &self.config.invalidation_log {
            publish_invalidations(path, purges)?;
        }
        Ok(purged)
    }

    /// Applies purges other processes appended to the invalidation log.
    pub fn synchronize(&self, force: bool) {
        let Some(path) = self.config.invalidation_log.as_deref() else {
            return;
        };
        let now = self.started.elapsed().as_millis() as u64;
        let next = self.next_log_poll.load(Ordering::Relaxed);
        if !force
            && (now < next
                || self
                    .next_log_poll
                    .compare_exchange(
                        next,
                        now + INVALIDATION_POLL.as_millis() as u64,
                        Ordering::Relaxed,
                        Ordering::Relaxed,
                    )
                    .is_err())
        {
            return;
        }
        let purges = {
            let mut cursor = lock(&self.log_cursor);
            match read_invalidations(
                path,
                &mut cursor,
                &self.config.requests.ignored_query_params,
            ) {
                Some(purges) => purges,
                None => return,
            }
        };
        self.purge(&purges);
    }

    pub fn stats(&self) -> Stats {
        let (entries, bytes) = {
            let inner = self.read();
            (inner.entries.len() as u64, inner.bytes as u64)
        };
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        Stats {
            hits: load(&self.counters.hits),
            misses: load(&self.counters.misses),
            stale: load(&self.counters.stale),
            collapsed: load(&self.counters.collapsed),
            purges: load(&self.counters.purges),
            purged_entries: load(&self.counters.purged_entries),
            stores: load(&self.counters.stores),
            evictions: load(&self.counters.evictions),
            not_modified: load(&self.counters.not_modified),
            revalidations: load(&self.counters.revalidations),
            bypasses: load(&self.counters.bypasses),
            uncacheable: load(&self.counters.uncacheable),
            entries,
            bytes,
        }
    }
}

/// Cache metrics beyond the hit/miss/collapsed/stale/purge counters that the
/// main metrics block renders.
pub fn render_metrics(output: &mut String, stats: &Stats) {
    use std::fmt::Write as _;
    let _ = write!(
        output,
        concat!(
            "# TYPE pam_http_response_cache_entries gauge\n",
            "pam_http_response_cache_entries {}\n",
            "# TYPE pam_http_response_cache_bytes gauge\n",
            "pam_http_response_cache_bytes {}\n",
            "# TYPE pam_http_response_cache_stores_total counter\n",
            "pam_http_response_cache_stores_total {}\n",
            "# TYPE pam_http_response_cache_evictions_total counter\n",
            "pam_http_response_cache_evictions_total {}\n",
            "# TYPE pam_http_response_cache_not_modified_total counter\n",
            "pam_http_response_cache_not_modified_total {}\n",
            "# TYPE pam_http_response_cache_revalidations_total counter\n",
            "pam_http_response_cache_revalidations_total {}\n",
            "# TYPE pam_http_response_cache_bypass_total counter\n",
            "pam_http_response_cache_bypass_total {}\n",
            "# TYPE pam_http_response_cache_uncacheable_total counter\n",
            "pam_http_response_cache_uncacheable_total {}\n",
            "# TYPE pam_http_response_cache_purged_entries_total counter\n",
            "pam_http_response_cache_purged_entries_total {}\n",
        ),
        stats.entries,
        stats.bytes,
        stats.stores,
        stats.evictions,
        stats.not_modified,
        stats.revalidations,
        stats.bypasses,
        stats.uncacheable,
        stats.purged_entries,
    );
}

#[cfg(test)]
impl PageCache {
    /// Recomputes every bookkeeping structure and checks it against the
    /// incremental state and the configured bounds.
    pub fn check_invariants(&self) {
        let inner = self.read();
        let mut bytes = 0;
        for (id, entry) in &inner.entries {
            assert_eq!(*id, entry.id);
            bytes += entry.size
                + entry.encoded.get().map_or(0, |encoded| {
                    encoded.gzip.as_ref().map_or(0, Bytes::len)
                        + encoded.brotli.as_ref().map_or(0, Bytes::len)
                });
            assert!(inner.primaries[&entry.primary].contains(id));
            assert_eq!(
                inner.order.get(&entry.ordered_at.load(Ordering::Relaxed)),
                Some(id),
                "every entry is filed once in the LRU order"
            );
        }
        assert_eq!(inner.order.len(), inner.entries.len());
        assert_eq!(
            inner.primaries.values().map(Vec::len).sum::<usize>(),
            inner.entries.len()
        );
        assert!(
            inner
                .primaries
                .values()
                .all(|ids| !ids.is_empty() && ids.len() <= MAX_VARIANTS)
        );
        assert_eq!(bytes, inner.bytes, "byte accounting drifted");
        assert!(inner.bytes <= self.config.max_bytes, "byte bound exceeded");
        assert!(
            inner.entries.len() <= self.config.max_entries,
            "entry bound exceeded"
        );
        assert!(inner.uncacheable.len() <= self.config.max_entries);
    }

    pub fn flights_in_progress(&self) -> usize {
        lock(&self.flights).len()
    }
}

fn vary_lists(headers: &HeaderMap, name: &str) -> bool {
    headers
        .get_all(http::header::VARY)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|listed| listed.trim().eq_ignore_ascii_case(name) || listed.trim() == "*")
}

impl Inner {
    fn insert(&mut self, entry: Arc<Entry>) {
        self.bytes += entry.size;
        self.order
            .insert(entry.ordered_at.load(Ordering::Relaxed), entry.id);
        self.primaries
            .entry(entry.primary.clone())
            .or_default()
            .push(entry.id);
        self.entries.insert(entry.id, entry);
    }

    fn remove(&mut self, id: u64) {
        let Some(entry) = self.entries.remove(&id) else {
            return;
        };
        self.bytes -= entry.size
            + entry.encoded.get().map_or(0, |encoded| {
                encoded.gzip.as_ref().map_or(0, Bytes::len)
                    + encoded.brotli.as_ref().map_or(0, Bytes::len)
            });
        if let Some(ids) = self.primaries.get_mut(&entry.primary) {
            ids.retain(|candidate| *candidate != id);
            if ids.is_empty() {
                self.primaries.remove(&entry.primary);
            }
        }
        let tick = entry.ordered_at.load(Ordering::Relaxed);
        if self.order.get(&tick) == Some(&id) {
            self.order.remove(&tick);
        }
    }
}

fn compress(body: &[u8]) -> Encoded {
    use std::io::Write as _;
    let gzip = {
        let mut encoder = flate2::write::GzEncoder::new(
            Vec::with_capacity(body.len() / 3),
            flate2::Compression::new(6),
        );
        encoder
            .write_all(body)
            .and_then(|()| encoder.finish())
            .ok()
            .filter(|compressed| compressed.len() < body.len())
            .map(Bytes::from)
    };
    let brotli = {
        let mut output = Vec::with_capacity(body.len() / 3);
        let parameters = brotli::enc::BrotliEncoderParams {
            quality: 5,
            lgwin: 22,
            ..Default::default()
        };
        brotli::BrotliCompress(&mut &body[..], &mut output, &parameters)
            .ok()
            .filter(|_| output.len() < body.len())
            .map(|_| Bytes::from(output))
    };
    Encoded { gzip, brotli }
}

/// Collects a response body up to `limit` bytes. When it is larger, returns
/// a body that replays what was read followed by the rest, unbuffered.
pub async fn collect_limited(body: Body, limit: usize) -> Result<Bytes, Body> {
    use futures_util::StreamExt;
    if body.size_hint().lower() > limit as u64 {
        return Err(body);
    }
    let mut stream = body.into_data_stream();
    let mut chunks: Vec<Bytes> = Vec::new();
    let mut total = 0usize;
    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(chunk) => {
                total += chunk.len();
                chunks.push(chunk);
                if total > limit {
                    let replay =
                        futures_util::stream::iter(chunks.into_iter().map(Ok::<_, axum::Error>));
                    return Err(Body::from_stream(replay.chain(stream)));
                }
            }
            Err(error) => {
                let replay = futures_util::stream::iter(
                    chunks
                        .into_iter()
                        .map(Ok::<_, axum::Error>)
                        .chain(std::iter::once(Err(error))),
                );
                return Err(Body::from_stream(replay));
            }
        }
    }
    if chunks.len() == 1 {
        return Ok(chunks.pop().unwrap_or_default());
    }
    let mut joined = bytes::BytesMut::with_capacity(total);
    for chunk in chunks {
        joined.extend_from_slice(&chunk);
    }
    Ok(joined.freeze())
}

fn file_identity(metadata: &std::fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (metadata.dev(), metadata.ino())
}

/// Reads purges appended since the cursor. A rotated (different file) or
/// truncated log purges everything: lines may have been missed.
fn read_invalidations(
    path: &Path,
    cursor: &mut LogCursor,
    ignored: &[String],
) -> Option<Vec<Purge>> {
    let file = OpenOptions::new().read(true).open(path).ok()?;
    let metadata = file.metadata().ok()?;
    let identity = file_identity(&metadata);
    let mut purges = Vec::new();
    match cursor.identity {
        Some(known) if known != identity => {
            purges.push(Purge::All);
            cursor.offset = 0;
        }
        _ if metadata.len() < cursor.offset => {
            purges.push(Purge::All);
            cursor.offset = 0;
        }
        _ => {}
    }
    cursor.identity = Some(identity);
    if metadata.len() == cursor.offset {
        return Some(purges);
    }
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(cursor.offset)).ok()?;
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            // A line still being written is consumed on the next poll.
            Ok(_) if !line.ends_with('\n') => break,
            Ok(read) => {
                cursor.offset += read as u64;
                if let Some(purge) = Purge::parse_line(&line, ignored) {
                    if purge == Purge::All {
                        purges.clear();
                        purges.push(Purge::All);
                    } else if !purges.contains(&Purge::All) {
                        purges.push(purge);
                    }
                }
            }
            Err(_) => break,
        }
    }
    Some(purges)
}

/// Appends purges to the shared invalidation log (rotating it when large).
pub fn publish_invalidations(path: &Path, purges: &[Purge]) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    if purges.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| {
            format!(
                "cannot open cache invalidation log {}: {error}",
                path.display()
            )
        })?;
    let mut record = String::new();
    for purge in purges {
        record.push_str(&purge.to_line());
        record.push('\n');
    }
    file.write_all(record.as_bytes())
        .map_err(|error| format!("cannot publish cache invalidation: {error}"))?;
    if file
        .metadata()
        .is_ok_and(|metadata| metadata.len() > INVALIDATION_LOG_MAX_BYTES)
    {
        let rotated = path.with_extension(format!("rotate-{}", std::process::id()));
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&rotated)
            .and_then(|_| std::fs::rename(&rotated, path))
            .map_err(|error| format!("cannot rotate cache invalidation log: {error}"))?;
    }
    Ok(())
}

/// Default invalidation log of a Laravel application (pam-octane's default).
pub const LARAVEL_INVALIDATION_LOG: &str = "storage/framework/cache/pam-page-cache.log";

/// `pam cache:purge [--tag=T]... [--url=U]... [--all] [--log=PATH]`
///
/// Appends purges to the invalidation log every worker of the application
/// tails, so running servers drop matching pages within ~100 ms.
pub fn cli(arguments: impl Iterator<Item = std::ffi::OsString>) -> Result<u8, String> {
    let mut purges = Vec::new();
    let mut log = None;
    let mut arguments = arguments.map(|argument| argument.to_string_lossy().into_owned());
    let mut raw_urls = Vec::new();
    while let Some(argument) = arguments.next() {
        let (name, inline) = match argument.split_once('=') {
            Some((name, value)) => (name.to_owned(), Some(value.to_owned())),
            None => (argument.clone(), None),
        };
        let mut value = |name: &str| {
            inline
                .clone()
                .or_else(|| arguments.next())
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match name.as_str() {
            "--all" if inline.is_none() => purges.push(Purge::All),
            "--tag" | "--tags" => {
                for tag in value(&name)?
                    .split(',')
                    .map(str::trim)
                    .filter(|tag| !tag.is_empty())
                {
                    if !policy::valid_tag(tag) {
                        return Err(format!(
                            "invalid cache tag {tag:?}: use letters, digits and - _ . : /"
                        ));
                    }
                    purges.push(Purge::Tag(tag.to_owned()));
                }
            }
            "--url" => raw_urls.push(value(&name)?),
            "--log" => log = Some(PathBuf::from(value(&name)?)),
            "-h" | "--help" => {
                println!(
                    "Usage: pam cache:purge [--tag=TAG[,TAG]]... [--url=URL]... [--all] [--log=PATH]\n\n\
                     Purges the PAM edge page cache of a running application.\n\
                     URLs may be absolute or paths; a trailing * purges a path prefix.\n\
                     The log defaults to $PAM_CACHE_INVALIDATION_LOG, then {LARAVEL_INVALIDATION_LOG}\n\
                     in a Laravel application (the responseCacheInvalidationLog option)."
                );
                return Ok(0);
            }
            other => return Err(format!("unknown cache:purge option {other}")),
        }
    }
    for url in raw_urls {
        let pattern = policy::UrlPattern::parse(&url, &[]).ok_or_else(|| {
            format!("invalid URL {url:?}: use /path, /prefix* or http(s)://host/path")
        })?;
        purges.push(Purge::Url(pattern));
    }
    if purges.is_empty() {
        return Err("nothing to purge: pass --tag, --url or --all".to_owned());
    }
    let log = log
        .or_else(|| std::env::var_os("PAM_CACHE_INVALIDATION_LOG").map(PathBuf::from))
        .or_else(|| {
            Path::new("artisan")
                .is_file()
                .then(|| PathBuf::from(LARAVEL_INVALIDATION_LOG))
        })
        .ok_or_else(|| {
            "cannot find the invalidation log: pass --log PATH (the server's responseCacheInvalidationLog)"
                .to_owned()
        })?;
    publish_invalidations(&log, &purges)?;
    println!("Published {} purge(s) to {}", purges.len(), log.display());
    Ok(0)
}

static GLOBAL: RwLock<Option<Arc<PageCache>>> = RwLock::new(None);

/// Makes `cache` the target of PHP's `pam_cache_purge()` in this process.
pub fn register_global(cache: &Arc<PageCache>) {
    *GLOBAL
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(cache.clone());
    crate::php::register_cache_purge_handler(native_purge);
}

/// Called by PHP's `pam_cache_purge()` on the PHP thread with newline
/// separated invalidation lines. Returns the number of entries purged in this
/// worker, -1 when no page cache runs in this process, -2 for invalid input
/// and -3 when the invalidation log could not be written.
extern "C" fn native_purge(lines: *const std::os::raw::c_char, length: usize) -> i64 {
    if lines.is_null() {
        return -2;
    }
    // SAFETY: the C caller passes a buffer of `length` bytes it owns for the
    // duration of the call.
    let bytes = unsafe { std::slice::from_raw_parts(lines.cast::<u8>(), length) };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return -2;
    };
    let Some(cache) = GLOBAL
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    else {
        return -1;
    };
    let mut purges = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        match Purge::parse_line(line, &cache.config.requests.ignored_query_params) {
            Some(purge) => purges.push(purge),
            None => return -2,
        }
    }
    match cache.purge_everywhere(&purges) {
        Ok(purged) => purged as i64,
        Err(error) => {
            eprintln!("pam: {error}");
            -3
        }
    }
}

#[cfg(test)]
mod tests;
