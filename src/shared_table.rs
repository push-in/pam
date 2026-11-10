//! Shared-memory tables: fixed-size hash tables that several PHP worker
//! processes read and write concurrently, with `Swoole\Table` semantics.
//!
//! A table lives in one memory segment sized when it is opened (header, lock
//! stripes, bucket heads and a fixed pool of rows). An all-zero segment is a
//! valid empty table, so a segment needs no initialization pass and its pages
//! are only committed as rows are used.
//!
//! Concurrency: each bucket belongs to one of up to 256 lock stripes. A stripe
//! lock is a word holding the owner's thread id; every operation holds at most one
//! stripe at a time and never calls back into PHP while holding it. Free rows
//! form a tagged lock-free stack.
//!
//! Crash safety: a worker can be killed at any instruction (`SIGKILL`, OOM).
//! A waiter that finds a stripe owned by a dead process takes it over and
//! repairs it: rows whose write was interrupted are dropped (like an evicted
//! cache entry), broken chains are cut and the stripe's count is recomputed.
//! A row that was allocated but not yet linked when its writer died is
//! reclaimed by a full collection the next time the table runs out of rows.
//! No interrupted write is ever visible to a reader.

use std::ptr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering, fence};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const MAGIC: u64 = 0x5041_4d5f_5442_4c31; // "PAM_TBL1"
const HEADER_BYTES: usize = 128;
const STRIPE_BYTES: usize = 64;
const MAX_STRIPES: usize = 256;
const ROW_HEADER_BYTES: usize = 24;
pub const MAX_ROWS: u64 = 1 << 26;
pub const MAX_KEY_BYTES: u32 = 1024;
pub const MAX_VALUE_BYTES: u32 = 16 * 1024 * 1024;
pub const MAX_SEGMENT_BYTES: u64 = 1 << 40;

const STATE_FREE: u32 = 0;
const STATE_LIVE: u32 = 1;
const STATE_WRITING: u32 = 2;

#[repr(C)]
struct Header {
    magic: AtomicU64,
    layout: AtomicU64,
    seed: AtomicU64,
    /// Rows ever handed out; rows at and above it are untouched zero pages.
    bump: AtomicU32,
    gc_lock: AtomicU32,
    /// `(tag << 32) | (row + 1)`; zero is the empty stack.
    free_head: AtomicU64,
    /// Repairs of stripes left locked by a dead process (diagnostics).
    recoveries: AtomicU64,
    /// Non-zero when a dead writer may have leaked an unlinked row.
    leak_suspected: AtomicU64,
}

#[repr(C)]
struct Stripe {
    lock: AtomicU32,
    count: AtomicU32,
}

#[repr(C)]
struct RowHeader {
    state: AtomicU32,
    /// Next row in the bucket chain, or in the free stack (`row + 1`, 0 = end).
    next: AtomicU32,
    hash: AtomicU64,
    key_length: AtomicU32,
    _reserved: u32,
}

const _: () = assert!(std::mem::size_of::<Header>() <= HEADER_BYTES);
const _: () = assert!(std::mem::size_of::<RowHeader>() == ROW_HEADER_BYTES);

/// The geometry of a table, derived from its declared size and row layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub capacity: u32,
    pub key_size: u32,
    pub value_size: u32,
    pub schema: u64,
    stripes: u32,
    row_stride: usize,
    buckets_offset: usize,
    rows_offset: usize,
    pub segment_bytes: usize,
}

impl Layout {
    /// `rows` is rounded up to a power of two, like `Swoole\Table`.
    pub fn new(rows: u64, key_size: u32, value_size: u32, schema: u64) -> Result<Self, String> {
        if rows == 0 || rows > MAX_ROWS {
            return Err(format!("table size must be between 1 and {MAX_ROWS} rows"));
        }
        if key_size == 0 || key_size > MAX_KEY_BYTES {
            return Err(format!(
                "table key size must be between 1 and {MAX_KEY_BYTES} bytes"
            ));
        }
        if value_size > MAX_VALUE_BYTES {
            return Err(format!("table rows cannot exceed {MAX_VALUE_BYTES} bytes"));
        }
        let capacity = rows.next_power_of_two();
        let stripes = capacity.min(MAX_STRIPES as u64) as usize;
        let row_stride = ROW_HEADER_BYTES + align8(key_size as usize) + align8(value_size as usize);
        let buckets_offset = HEADER_BYTES + stripes * STRIPE_BYTES;
        let rows_offset = align64(buckets_offset + capacity as usize * 4);
        let segment_bytes = (rows_offset as u64)
            .checked_add(
                capacity
                    .checked_mul(row_stride as u64)
                    .ok_or("table is too large")?,
            )
            .ok_or("table is too large")?;
        if segment_bytes > MAX_SEGMENT_BYTES {
            return Err(format!(
                "table would need {segment_bytes} bytes of shared memory"
            ));
        }
        let segment_bytes = align_page(segment_bytes as usize);
        Ok(Self {
            capacity: capacity as u32,
            key_size,
            value_size,
            schema,
            stripes: stripes as u32,
            row_stride,
            buckets_offset,
            rows_offset,
            segment_bytes,
        })
    }

    fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for part in [
            u64::from(self.capacity),
            u64::from(self.key_size),
            u64::from(self.value_size),
            self.schema,
        ] {
            for byte in part.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash | 1
    }
}

fn align8(value: usize) -> usize {
    (value + 7) & !7
}

fn align64(value: usize) -> usize {
    (value + 63) & !63
}

fn align_page(value: usize) -> usize {
    (value + 4095) & !4095
}

/// A mapped segment. Unmapped when the last table using it is dropped.
pub struct Mapping {
    base: *mut u8,
    length: usize,
    #[cfg(not(unix))]
    _heap: Vec<u64>,
}

// SAFETY: the mapping is plain shared memory; all access goes through atomics
// or through row bytes guarded by stripe locks.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}

impl Mapping {
    /// A segment shared with processes forked after this call.
    pub fn anonymous(length: usize) -> Result<Self, String> {
        #[cfg(unix)]
        {
            // SAFETY: anonymous shared mapping; no file descriptor involved.
            let base = unsafe {
                libc::mmap(
                    ptr::null_mut(),
                    length,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            if base == libc::MAP_FAILED {
                return Err(format!(
                    "cannot map {length} bytes of shared memory: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(Self {
                base: base.cast(),
                length,
            })
        }
        #[cfg(not(unix))]
        {
            let mut heap = vec![0_u64; length.div_ceil(8)];
            Ok(Self {
                base: heap.as_mut_ptr().cast(),
                length,
                _heap: heap,
            })
        }
    }

    /// Maps a segment received from the PAM master.
    #[cfg(unix)]
    pub fn from_fd(fd: std::os::fd::RawFd, length: usize) -> Result<Self, String> {
        // SAFETY: fd is a memory file of at least `length` bytes.
        let base = unsafe {
            libc::mmap(
                ptr::null_mut(),
                length,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };
        if base == libc::MAP_FAILED {
            return Err(format!(
                "cannot map shared table segment: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self {
            base: base.cast(),
            length,
        })
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        #[cfg(unix)]
        // SAFETY: base/length come from a successful mmap.
        unsafe {
            libc::munmap(self.base.cast(), self.length);
        }
    }
}

/// Outcome of a write.
#[derive(Debug, PartialEq, Eq)]
pub enum Write {
    Stored,
    Full,
}

/// One column write: `bytes` copied to `offset` of the row value.
pub struct Patch<'a> {
    pub offset: u32,
    pub bytes: &'a [u8],
}

pub struct Table {
    mapping: Arc<Mapping>,
    layout: Layout,
    seed: u64,
}

// SAFETY: see Mapping.
unsafe impl Send for Table {}
unsafe impl Sync for Table {}

impl Table {
    /// Attaches to (and on first use stamps) a zero-filled or existing segment.
    pub fn attach(mapping: Arc<Mapping>, layout: Layout) -> Result<Self, String> {
        if mapping.length < layout.segment_bytes {
            return Err("shared table segment is smaller than its layout".to_owned());
        }
        let header = header(&mapping);
        let fingerprint = layout.fingerprint();
        match header
            .layout
            .compare_exchange(0, fingerprint, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => {}
            Err(existing) if existing == fingerprint => {}
            Err(_) => return Err("shared table segment has a different layout".to_owned()),
        }
        let candidate = random_seed() | 1;
        let seed =
            match header
                .seed
                .compare_exchange(0, candidate, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => candidate,
                Err(existing) => existing,
            };
        header.magic.store(MAGIC, Ordering::Release);
        current_owner();
        Ok(Self {
            mapping,
            layout,
            seed,
        })
    }

    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    pub fn memory_size(&self) -> usize {
        self.layout.segment_bytes
    }

    pub fn recoveries(&self) -> u64 {
        self.header().recoveries.load(Ordering::Relaxed)
    }

    pub fn count(&self) -> u64 {
        (0..self.layout.stripes)
            .map(|stripe| u64::from(self.stripe(stripe).count.load(Ordering::Relaxed)))
            .sum()
    }

    /// Writes the given columns of `key`; a new row starts zeroed.
    pub fn set(&self, key: &[u8], patches: &[Patch<'_>]) -> Result<Write, String> {
        self.check_key(key)?;
        for patch in patches {
            if patch.offset as usize + patch.bytes.len() > self.layout.value_size as usize {
                return Err("column write outside the row".to_owned());
            }
        }
        let hash = self.hash(key);
        for attempt in 0..2 {
            let guard = self.lock_bucket(hash);
            if let Some(row) = self.find(guard.bucket, hash, key) {
                let header = self.row_header(row);
                header.state.store(STATE_WRITING, Ordering::Relaxed);
                fence(Ordering::Release);
                self.apply(row, patches);
                header.state.store(STATE_LIVE, Ordering::Release);
                return Ok(Write::Stored);
            }
            let Some(row) = self.allocate() else {
                drop(guard);
                if attempt == 0 && self.header().leak_suspected.load(Ordering::Acquire) != 0 {
                    self.collect();
                    continue;
                }
                return Ok(Write::Full);
            };
            self.insert(&guard, row, hash, key, patches);
            return Ok(Write::Stored);
        }
        Ok(Write::Full)
    }

    /// Copies the row value of `key` into `output` (`value_size` bytes).
    pub fn get(&self, key: &[u8], output: &mut [u8]) -> bool {
        if self.check_key(key).is_err() || output.len() < self.layout.value_size as usize {
            return false;
        }
        let hash = self.hash(key);
        let guard = self.lock_bucket(hash);
        let Some(row) = self.find(guard.bucket, hash, key) else {
            return false;
        };
        // SAFETY: value region is inside the mapping and the stripe is locked.
        unsafe {
            ptr::copy_nonoverlapping(
                self.value_ptr(row),
                output.as_mut_ptr(),
                self.layout.value_size as usize,
            );
        }
        true
    }

    pub fn exists(&self, key: &[u8]) -> bool {
        if self.check_key(key).is_err() {
            return false;
        }
        let hash = self.hash(key);
        let guard = self.lock_bucket(hash);
        self.find(guard.bucket, hash, key).is_some()
    }

    pub fn delete(&self, key: &[u8]) -> bool {
        if self.check_key(key).is_err() {
            return false;
        }
        let hash = self.hash(key);
        let guard = self.lock_bucket(hash);
        let head = self.bucket(guard.bucket);
        let mut previous: Option<u32> = None;
        let mut link = head.load(Ordering::Acquire);
        let mut steps = 0;
        while link != 0 && steps <= self.layout.capacity {
            let row = link - 1;
            let header = self.row_header(row);
            let next = header.next.load(Ordering::Acquire);
            if header.hash.load(Ordering::Relaxed) == hash && self.key_matches(row, key) {
                match previous {
                    Some(previous) => self
                        .row_header(previous)
                        .next
                        .store(next, Ordering::Release),
                    None => head.store(next, Ordering::Release),
                }
                self.stripe(guard.stripe)
                    .count
                    .fetch_sub(1, Ordering::Relaxed);
                self.release_row(row);
                return true;
            }
            previous = Some(row);
            link = next;
            steps += 1;
        }
        false
    }

    /// Adds `delta` to an 8-byte integer column, creating a zeroed row when
    /// `key` is missing, and returns the new value. The update is a single
    /// aligned store, so it never leaves a torn value behind.
    pub fn increment_int(
        &self,
        key: &[u8],
        offset: u32,
        delta: i64,
    ) -> Result<Option<i64>, String> {
        self.increment(key, offset, |cell| {
            let value = i64::from_ne_bytes(cell).wrapping_add(delta);
            (value.to_ne_bytes(), value)
        })
    }

    pub fn increment_float(
        &self,
        key: &[u8],
        offset: u32,
        delta: f64,
    ) -> Result<Option<f64>, String> {
        self.increment(key, offset, |cell| {
            let value = f64::from_ne_bytes(cell) + delta;
            (value.to_ne_bytes(), value)
        })
    }

    fn increment<T>(
        &self,
        key: &[u8],
        offset: u32,
        update: impl Fn([u8; 8]) -> ([u8; 8], T),
    ) -> Result<Option<T>, String> {
        self.check_key(key)?;
        if offset % 8 != 0 || offset as usize + 8 > self.layout.value_size as usize {
            return Err("numeric column outside the row".to_owned());
        }
        let hash = self.hash(key);
        for attempt in 0..2 {
            let guard = self.lock_bucket(hash);
            let row = match self.find(guard.bucket, hash, key) {
                Some(row) => row,
                None => {
                    let Some(row) = self.allocate() else {
                        drop(guard);
                        if attempt == 0 && self.header().leak_suspected.load(Ordering::Acquire) != 0
                        {
                            self.collect();
                            continue;
                        }
                        return Ok(None);
                    };
                    self.insert(&guard, row, hash, key, &[]);
                    row
                }
            };
            // SAFETY: offset is 8-aligned within the 8-aligned value region.
            let cell = unsafe { &*(self.value_ptr(row).add(offset as usize) as *const AtomicU64) };
            let (bytes, value) = update(cell.load(Ordering::Relaxed).to_ne_bytes());
            cell.store(u64::from_ne_bytes(bytes), Ordering::Release);
            return Ok(Some(value));
        }
        Ok(None)
    }

    /// Copies the rows of buckets starting at `*cursor` into `output` as
    /// `[u32 key length][key][value]` records, stopping after `limit` rows or
    /// at the end. Returns the number of rows; `*cursor` is the next bucket and
    /// equals the bucket count at the end.
    pub fn scan(&self, cursor: &mut u64, limit: usize, output: &mut Vec<u8>) -> usize {
        let mut rows = 0;
        let value_size = self.layout.value_size as usize;
        while *cursor < u64::from(self.layout.capacity) && rows < limit {
            let bucket = *cursor as u32;
            *cursor += 1;
            let _guard = self.lock_stripe(bucket & (self.layout.stripes - 1), bucket);
            let mut link = self.bucket(bucket).load(Ordering::Acquire);
            let mut steps = 0;
            while link != 0 && steps <= self.layout.capacity {
                let row = link - 1;
                let length = self.row_header(row).key_length.load(Ordering::Relaxed) as usize;
                output.extend_from_slice(&(length as u32).to_ne_bytes());
                // SAFETY: key and value regions are inside the locked row.
                unsafe {
                    output.extend_from_slice(std::slice::from_raw_parts(self.key_ptr(row), length));
                    output.extend_from_slice(std::slice::from_raw_parts(
                        self.value_ptr(row),
                        value_size,
                    ));
                }
                rows += 1;
                link = self.row_header(row).next.load(Ordering::Acquire);
                steps += 1;
            }
        }
        rows
    }

    /// Reclaims rows leaked by writers that died between allocating and
    /// linking a row. Holds every stripe while it runs.
    pub fn collect(&self) {
        let header = self.header();
        let _gc = PidLock::acquire(&header.gc_lock, |_| {});
        let guards: Vec<StripeGuard<'_>> = (0..self.layout.stripes)
            .map(|stripe| self.lock_stripe(stripe, stripe))
            .collect();
        let bump = header
            .bump
            .load(Ordering::Acquire)
            .min(self.layout.capacity);
        let mut reachable = vec![false; bump as usize];
        for bucket in 0..self.layout.capacity {
            let mut link = self.bucket(bucket).load(Ordering::Acquire);
            while link != 0 {
                let row = (link - 1) as usize;
                if row >= reachable.len() || reachable[row] {
                    break;
                }
                reachable[row] = true;
                link = self.row_header(row as u32).next.load(Ordering::Acquire);
            }
        }
        let mut head = 0_u32;
        for row in (0..bump).rev() {
            if !reachable[row as usize] {
                let row_header = self.row_header(row);
                row_header.state.store(STATE_FREE, Ordering::Relaxed);
                row_header.next.store(head, Ordering::Relaxed);
                head = row + 1;
            }
        }
        let tag = (header.free_head.load(Ordering::Acquire) >> 32).wrapping_add(1);
        header
            .free_head
            .store((tag << 32) | u64::from(head), Ordering::Release);
        header.leak_suspected.store(0, Ordering::Release);
        drop(guards);
    }

    fn insert(
        &self,
        guard: &StripeGuard<'_>,
        row: u32,
        hash: u64,
        key: &[u8],
        patches: &[Patch<'_>],
    ) {
        let header = self.row_header(row);
        header.state.store(STATE_WRITING, Ordering::Relaxed);
        header.hash.store(hash, Ordering::Relaxed);
        header.key_length.store(key.len() as u32, Ordering::Relaxed);
        // SAFETY: row is exclusively owned (popped from the free stack).
        unsafe {
            ptr::copy_nonoverlapping(key.as_ptr(), self.key_ptr(row), key.len());
            ptr::write_bytes(self.value_ptr(row), 0, self.layout.value_size as usize);
        }
        self.apply(row, patches);
        let head = self.bucket(guard.bucket);
        header
            .next
            .store(head.load(Ordering::Relaxed), Ordering::Relaxed);
        header.state.store(STATE_LIVE, Ordering::Release);
        // Linking is the commit point.
        head.store(row + 1, Ordering::Release);
        self.stripe(guard.stripe)
            .count
            .fetch_add(1, Ordering::Relaxed);
    }

    fn apply(&self, row: u32, patches: &[Patch<'_>]) {
        let value = self.value_ptr(row);
        for patch in patches {
            // SAFETY: bounds were checked against value_size by the caller.
            unsafe {
                ptr::copy_nonoverlapping(
                    patch.bytes.as_ptr(),
                    value.add(patch.offset as usize),
                    patch.bytes.len(),
                );
            }
        }
    }

    fn find(&self, bucket: u32, hash: u64, key: &[u8]) -> Option<u32> {
        let mut link = self.bucket(bucket).load(Ordering::Acquire);
        let mut steps = 0;
        while link != 0 && steps <= self.layout.capacity {
            let row = link - 1;
            let header = self.row_header(row);
            if header.hash.load(Ordering::Relaxed) == hash && self.key_matches(row, key) {
                return Some(row);
            }
            link = header.next.load(Ordering::Acquire);
            steps += 1;
        }
        None
    }

    fn key_matches(&self, row: u32, key: &[u8]) -> bool {
        let header = self.row_header(row);
        if header.key_length.load(Ordering::Relaxed) as usize != key.len() {
            return false;
        }
        // SAFETY: key region holds key_size >= key.len() bytes.
        unsafe { std::slice::from_raw_parts(self.key_ptr(row), key.len()) == key }
    }

    fn allocate(&self) -> Option<u32> {
        let header = self.header();
        loop {
            let head = header.free_head.load(Ordering::Acquire);
            let link = head as u32;
            if link == 0 {
                break;
            }
            let row = link - 1;
            if row >= self.layout.capacity {
                // Only a corrupted stack can get here; abandon it.
                header.leak_suspected.store(1, Ordering::Release);
                let _ =
                    header
                        .free_head
                        .compare_exchange(head, 0, Ordering::AcqRel, Ordering::Relaxed);
                break;
            }
            let next = self.row_header(row).next.load(Ordering::Acquire);
            let tagged = (((head >> 32).wrapping_add(1)) << 32) | u64::from(next);
            if header
                .free_head
                .compare_exchange_weak(head, tagged, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return Some(row);
            }
        }
        let mut bump = header.bump.load(Ordering::Relaxed);
        while bump < self.layout.capacity {
            match header.bump.compare_exchange_weak(
                bump,
                bump + 1,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(bump),
                Err(current) => bump = current,
            }
        }
        None
    }

    fn release_row(&self, row: u32) {
        let header = self.header();
        let row_header = self.row_header(row);
        row_header.state.store(STATE_FREE, Ordering::Relaxed);
        loop {
            let head = header.free_head.load(Ordering::Acquire);
            row_header.next.store(head as u32, Ordering::Relaxed);
            let tagged = (((head >> 32).wrapping_add(1)) << 32) | u64::from(row + 1);
            if header
                .free_head
                .compare_exchange_weak(head, tagged, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
        }
    }

    /// Repairs a stripe whose previous owner died while holding it.
    fn repair_stripe(&self, stripe: u32) {
        let header = self.header();
        header.recoveries.fetch_add(1, Ordering::Relaxed);
        header.leak_suspected.store(1, Ordering::Release);
        let bump = header
            .bump
            .load(Ordering::Acquire)
            .min(self.layout.capacity);
        let mut count = 0_u32;
        let mut bucket = stripe;
        while bucket < self.layout.capacity {
            let head = self.bucket(bucket);
            let mut previous: Option<u32> = None;
            let mut link = head.load(Ordering::Acquire);
            let mut steps = 0;
            while link != 0 {
                let row = link - 1;
                let valid = row < bump && steps < bump;
                let state = if valid {
                    self.row_header(row).state.load(Ordering::Acquire)
                } else {
                    STATE_FREE
                };
                let set_link = |value: u32| match previous {
                    Some(previous) => self
                        .row_header(previous)
                        .next
                        .store(value, Ordering::Release),
                    None => head.store(value, Ordering::Release),
                };
                if !valid {
                    set_link(0);
                    break;
                }
                let next = self.row_header(row).next.load(Ordering::Acquire);
                if state == STATE_LIVE
                    && self.row_header(row).hash.load(Ordering::Relaxed)
                        & u64::from(self.layout.capacity - 1)
                        == u64::from(bucket)
                {
                    count += 1;
                    previous = Some(row);
                } else {
                    // Interrupted write (or a row that does not belong here):
                    // unlink it; the collector returns it to the free stack.
                    set_link(next);
                }
                link = next;
                steps += 1;
            }
            bucket += self.layout.stripes;
        }
        self.stripe(stripe).count.store(count, Ordering::Relaxed);
    }

    fn lock_bucket(&self, hash: u64) -> StripeGuard<'_> {
        let bucket = (hash & u64::from(self.layout.capacity - 1)) as u32;
        self.lock_stripe(bucket & (self.layout.stripes - 1), bucket)
    }

    fn lock_stripe(&self, stripe: u32, bucket: u32) -> StripeGuard<'_> {
        let lock = PidLock::acquire(&self.stripe(stripe).lock, |_| self.repair_stripe(stripe));
        StripeGuard {
            _lock: lock,
            stripe,
            bucket,
        }
    }

    fn check_key(&self, key: &[u8]) -> Result<(), String> {
        if key.len() > self.layout.key_size as usize {
            return Err(format!(
                "table keys cannot exceed {} bytes",
                self.layout.key_size
            ));
        }
        Ok(())
    }

    fn hash(&self, key: &[u8]) -> u64 {
        // SipHash-1-3 keyed by the segment's random seed: chains stay short
        // even for keys chosen by an attacker.
        let mut hasher = SeededHasher::new(self.seed);
        hasher.write(key);
        hasher.finish()
    }

    fn header(&self) -> &Header {
        header(&self.mapping)
    }

    fn stripe(&self, stripe: u32) -> &Stripe {
        // SAFETY: stripe < stripes; the stripe array follows the header.
        unsafe {
            &*(self
                .mapping
                .base
                .add(HEADER_BYTES + stripe as usize * STRIPE_BYTES) as *const Stripe)
        }
    }

    fn bucket(&self, bucket: u32) -> &AtomicU32 {
        // SAFETY: bucket < capacity.
        unsafe {
            &*(self
                .mapping
                .base
                .add(self.layout.buckets_offset + bucket as usize * 4)
                as *const AtomicU32)
        }
    }

    fn row_base(&self, row: u32) -> *mut u8 {
        // SAFETY: row < capacity.
        unsafe {
            self.mapping
                .base
                .add(self.layout.rows_offset + row as usize * self.layout.row_stride)
        }
    }

    fn row_header(&self, row: u32) -> &RowHeader {
        // SAFETY: rows are 8-aligned and start with a RowHeader.
        unsafe { &*(self.row_base(row) as *const RowHeader) }
    }

    fn key_ptr(&self, row: u32) -> *mut u8 {
        // SAFETY: within the row.
        unsafe { self.row_base(row).add(ROW_HEADER_BYTES) }
    }

    fn value_ptr(&self, row: u32) -> *mut u8 {
        // SAFETY: within the row.
        unsafe { self.key_ptr(row).add(align8(self.layout.key_size as usize)) }
    }
}

fn header(mapping: &Mapping) -> &Header {
    // SAFETY: every mapping holds at least HEADER_BYTES.
    unsafe { &*(mapping.base as *const Header) }
}

struct StripeGuard<'a> {
    _lock: PidLock<'a>,
    stripe: u32,
    bucket: u32,
}

/// A process-shared lock word holding its owner's thread id (0 = free). A waiter
/// takes the lock over from an owner that no longer exists and runs `repair`.
struct PidLock<'a> {
    word: &'a AtomicU32,
}

impl<'a> PidLock<'a> {
    fn acquire(word: &'a AtomicU32, repair: impl FnOnce(u32)) -> Self {
        let me = current_owner();
        let mut spins = 0_u32;
        loop {
            match word.compare_exchange_weak(0, me, Ordering::Acquire, Ordering::Relaxed) {
                Ok(_) => return Self { word },
                Err(0) => continue,
                Err(owner) => {
                    spins = spins.saturating_add(1);
                    if spins < 128 {
                        std::hint::spin_loop();
                        continue;
                    }
                    if spins < 160 {
                        std::thread::yield_now();
                        continue;
                    }
                    // Critical sections are a few microseconds; check the owner
                    // about every millisecond.
                    if owner_lost(owner, me)
                        && word
                            .compare_exchange(owner, me, Ordering::Acquire, Ordering::Relaxed)
                            .is_ok()
                    {
                        repair(owner);
                        return Self { word };
                    }
                    std::thread::sleep(Duration::from_micros(50));
                    spins = 140;
                }
            }
        }
    }
}

impl Drop for PidLock<'_> {
    fn drop(&mut self) {
        self.word.store(0, Ordering::Release);
    }
}

fn process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // SAFETY: signal 0 only checks for existence and permission.
        if unsafe { libc::kill(pid as libc::pid_t, 0) } == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

static FORK_GENERATION: AtomicU32 = AtomicU32::new(0);

thread_local! {
    static OWNER: std::cell::Cell<(u32, u32)> = const { std::cell::Cell::new((u32::MAX, 0)) };
}

/// The lock owner id of the calling thread: its kernel thread id on Linux
/// (unique system-wide, so threads of one process exclude each other and a
/// dead owner is detectable), the PID elsewhere. Refreshed after `fork`.
fn current_owner() -> u32 {
    static ATFORK: OnceLock<()> = OnceLock::new();
    ATFORK.get_or_init(|| {
        #[cfg(unix)]
        {
            extern "C" fn forked() {
                FORK_GENERATION.fetch_add(1, Ordering::Relaxed);
            }
            // SAFETY: registers a child handler that only bumps an atomic.
            unsafe {
                libc::pthread_atfork(None, None, Some(forked));
            }
        }
    });
    let generation = FORK_GENERATION.load(Ordering::Relaxed);
    OWNER.with(|owner| {
        let (cached_generation, id) = owner.get();
        if cached_generation == generation && id != 0 {
            return id;
        }
        #[cfg(target_os = "linux")]
        // SAFETY: gettid has no preconditions.
        let id = unsafe { libc::gettid() } as u32;
        #[cfg(not(target_os = "linux"))]
        let id = std::process::id();
        owner.set((generation, id));
        id
    })
}

/// Whether a lock held by `owner` can be taken over: its owner is gone, or
/// (Linux) it is this very thread, which can only be a lock leaked by a
/// crash in a previous incarnation of this id.
fn owner_lost(owner: u32, me: u32) -> bool {
    (cfg!(target_os = "linux") && owner == me) || !process_alive(owner)
}

fn random_seed() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos()),
    );
    hasher.finish()
}

/// SipHash-1-3 with a 64-bit seed (k0 = seed, k1 = rotated seed).
struct SeededHasher {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    tail: u64,
    tail_length: usize,
    length: usize,
}

impl SeededHasher {
    fn new(seed: u64) -> Self {
        let k0 = seed;
        let k1 = seed.rotate_left(32) ^ 0x9e37_79b9_7f4a_7c15;
        Self {
            v0: k0 ^ 0x736f_6d65_7073_6575,
            v1: k1 ^ 0x646f_7261_6e64_6f6d,
            v2: k0 ^ 0x6c79_6765_6e65_7261,
            v3: k1 ^ 0x7465_6462_7974_6573,
            tail: 0,
            tail_length: 0,
            length: 0,
        }
    }

    fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(13) ^ self.v0;
        self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(16) ^ self.v2;
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(21) ^ self.v0;
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(17) ^ self.v2;
        self.v2 = self.v2.rotate_left(32);
    }

    fn compress(&mut self, block: u64) {
        self.v3 ^= block;
        self.round();
        self.v0 ^= block;
    }

    fn write(&mut self, bytes: &[u8]) {
        self.length += bytes.len();
        for &byte in bytes {
            self.tail |= u64::from(byte) << (8 * self.tail_length);
            self.tail_length += 1;
            if self.tail_length == 8 {
                let block = self.tail;
                self.compress(block);
                self.tail = 0;
                self.tail_length = 0;
            }
        }
    }

    fn finish(mut self) -> u64 {
        let block = ((self.length as u64 & 0xff) << 56) | self.tail;
        self.compress(block);
        self.v2 ^= 0xff;
        self.round();
        self.round();
        self.round();
        self.v0 ^ self.v1 ^ self.v2 ^ self.v3
    }
}

// ---------------------------------------------------------------------------
// Opening tables by name.

type Registry = Mutex<std::collections::HashMap<(String, usize), Arc<Mapping>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

/// Environment variable naming the master's shared-memory socket.
pub const SOCKET_ENV: &str = "PAM_SHARED_MEMORY_SOCKET";

/// Opens a table. An empty `name` is a private table shared only with
/// processes forked afterwards (`Swoole\Table` semantics). A named table is
/// shared by every worker of a PAM cluster: the master owns the segment, so it
/// survives worker recycling and reloads and is freed when the master stops.
/// Outside a cluster, a named table is shared within the process.
pub fn open(name: &str, layout: Layout) -> Result<Table, String> {
    if name.is_empty() {
        return Table::attach(Arc::new(Mapping::anonymous(layout.segment_bytes)?), layout);
    }
    validate_name(name)?;
    let key = (
        format!("{name}.{:016x}", layout.fingerprint()),
        layout.segment_bytes,
    );
    let mut registry = registry()
        .lock()
        .map_err(|_| "shared table registry poisoned")?;
    if let Some(mapping) = registry.get(&key) {
        return Table::attach(mapping.clone(), layout);
    }
    let mapping = Arc::new(match std::env::var_os(SOCKET_ENV) {
        #[cfg(all(target_os = "linux", not(fuzzing)))]
        Some(socket) => segment_client::request(std::path::Path::new(&socket), &key.0, key.1)?,
        _ => Mapping::anonymous(layout.segment_bytes)?,
    });
    registry.insert(key, mapping.clone());
    Table::attach(mapping, layout)
}

pub fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > 200
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
    {
        return Err("table names use 1 to 200 characters from [A-Za-z0-9._:-]".to_owned());
    }
    Ok(())
}

#[cfg(all(test, target_os = "linux"))]
pub(crate) use segment_client::request as segment_client_request;

#[cfg(all(target_os = "linux", not(fuzzing)))]
mod segment_client {
    use super::Mapping;
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;
    use std::path::Path;
    use std::time::Duration;

    pub fn request(socket: &Path, key: &str, length: usize) -> Result<Mapping, String> {
        let mut stream = UnixStream::connect(socket)
            .map_err(|error| format!("cannot reach the PAM master for shared memory: {error}"))?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .and_then(|()| stream.set_write_timeout(Some(Duration::from_secs(10))))
            .map_err(|error| error.to_string())?;
        stream
            .write_all(format!("PAMSHM1 {length} {key}\n").as_bytes())
            .map_err(|error| format!("cannot request shared memory: {error}"))?;
        let (reply, fd) = crate::shared_memory_server::receive_fd(stream.as_raw_fd())
            .map_err(|error| format!("cannot receive shared memory: {error}"))?;
        let text = String::from_utf8_lossy(&reply);
        let Some(fd) = fd else {
            return Err(format!(
                "the PAM master refused shared memory: {}",
                text.trim().trim_start_matches("ER ")
            ));
        };
        // SAFETY: the kernel installed a fresh descriptor for us.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        if !text.starts_with("OK") {
            return Err(format!("unexpected shared memory reply: {}", text.trim()));
        }
        Mapping::from_fd(fd.as_raw_fd(), length)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn table(rows: u64, value: u32) -> Table {
        open("", Layout::new(rows, 64, value, 7).unwrap()).unwrap()
    }

    fn int_patch(value: &i64) -> [Patch<'_>; 1] {
        [Patch {
            offset: 0,
            bytes: bytemuck(value),
        }]
    }

    fn bytemuck(value: &i64) -> &[u8] {
        // SAFETY: i64 is plain old data.
        unsafe { std::slice::from_raw_parts((value as *const i64).cast(), 8) }
    }

    fn read_int(table: &Table, key: &[u8]) -> Option<i64> {
        let mut out = vec![0_u8; table.layout().value_size as usize];
        table
            .get(key, &mut out)
            .then(|| i64::from_ne_bytes(out[..8].try_into().unwrap()))
    }

    #[test]
    fn layout_rounds_rows_like_swoole_and_bounds_inputs() {
        let layout = Layout::new(1000, 64, 16, 1).unwrap();
        assert_eq!(layout.capacity, 1024);
        assert_eq!(layout.segment_bytes % 4096, 0);
        assert!(Layout::new(0, 64, 16, 1).is_err());
        assert!(Layout::new(10, 0, 16, 1).is_err());
        assert!(Layout::new(10, 64, MAX_VALUE_BYTES + 1, 1).is_err());
        assert!(Layout::new(MAX_ROWS, 64, MAX_VALUE_BYTES, 1).is_err());
    }

    #[test]
    fn stores_reads_overwrites_and_deletes_rows() {
        let table = table(8, 16);
        assert_eq!(table.set(b"a", &int_patch(&5)).unwrap(), Write::Stored);
        assert_eq!(read_int(&table, b"a"), Some(5));
        assert_eq!(table.set(b"a", &int_patch(&6)).unwrap(), Write::Stored);
        assert_eq!(read_int(&table, b"a"), Some(6));
        assert_eq!(table.count(), 1);
        assert!(table.exists(b"a"));
        assert!(table.delete(b"a"));
        assert!(!table.delete(b"a"));
        assert!(!table.exists(b"a"));
        assert_eq!(table.count(), 0);
        assert!(table.set(&[b'k'; 65], &[]).is_err());
    }

    #[test]
    fn partial_writes_keep_other_columns_and_new_rows_start_zeroed() {
        let table = table(4, 16);
        table
            .set(
                b"k",
                &[Patch {
                    offset: 8,
                    bytes: &[1; 8],
                }],
            )
            .unwrap();
        let mut out = [9_u8; 16];
        assert!(table.get(b"k", &mut out));
        assert_eq!(out, [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1]);
        table
            .set(
                b"k",
                &[Patch {
                    offset: 0,
                    bytes: &[2; 8],
                }],
            )
            .unwrap();
        assert!(table.get(b"k", &mut out));
        assert_eq!(out, [2, 2, 2, 2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1]);
        assert!(
            table
                .set(
                    b"k",
                    &[Patch {
                        offset: 12,
                        bytes: &[0; 8]
                    }]
                )
                .is_err()
        );
    }

    #[test]
    fn fills_every_row_and_reuses_deleted_rows() {
        let table = table(1000, 8);
        for index in 0..1024_i64 {
            assert_eq!(
                table
                    .set(index.to_string().as_bytes(), &int_patch(&index))
                    .unwrap(),
                Write::Stored
            );
        }
        assert_eq!(table.count(), 1024);
        assert_eq!(table.set(b"overflow", &[]).unwrap(), Write::Full);
        assert!(table.delete(b"7"));
        assert_eq!(table.set(b"overflow", &[]).unwrap(), Write::Stored);
        for index in (0..1024_i64).filter(|index| *index != 7) {
            assert_eq!(read_int(&table, index.to_string().as_bytes()), Some(index));
        }
    }

    #[test]
    fn increments_integers_and_floats_creating_missing_rows() {
        let table = table(4, 16);
        assert_eq!(table.increment_int(b"c", 0, 2).unwrap(), Some(2));
        assert_eq!(table.increment_int(b"c", 0, -5).unwrap(), Some(-3));
        assert_eq!(table.increment_float(b"c", 8, 1.5).unwrap(), Some(1.5));
        assert_eq!(table.increment_float(b"c", 8, 1.0).unwrap(), Some(2.5));
        assert!(table.increment_int(b"c", 4, 1).is_err());
        assert!(table.increment_int(b"c", 16, 1).is_err());
    }

    #[test]
    fn scans_every_row_exactly_once() {
        let table = table(64, 8);
        for index in 0..50_i64 {
            table
                .set(format!("key-{index}").as_bytes(), &int_patch(&index))
                .unwrap();
        }
        let mut cursor = 0;
        let mut seen = HashMap::new();
        loop {
            let mut buffer = Vec::new();
            let rows = table.scan(&mut cursor, 7, &mut buffer);
            let mut offset = 0;
            for _ in 0..rows {
                let length =
                    u32::from_ne_bytes(buffer[offset..offset + 4].try_into().unwrap()) as usize;
                let key =
                    String::from_utf8(buffer[offset + 4..offset + 4 + length].to_vec()).unwrap();
                let value = i64::from_ne_bytes(
                    buffer[offset + 4 + length..offset + 12 + length]
                        .try_into()
                        .unwrap(),
                );
                assert!(seen.insert(key, value).is_none());
                offset += 4 + length + 8;
            }
            if cursor >= 64 {
                break;
            }
        }
        assert_eq!(seen.len(), 50);
        assert_eq!(seen["key-42"], 42);
    }

    /// Model-based randomized test: a long random operation sequence must
    /// agree with a HashMap at every step.
    #[test]
    fn random_operations_match_a_model() {
        let table = table(128, 16);
        let mut model: HashMap<Vec<u8>, (i64, i64)> = HashMap::new();
        let mut state = 0x2545_f491_4f6c_dd1d_u64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..200_000 {
            let key = format!("k{}", next() % 200).into_bytes();
            match next() % 5 {
                0 | 1 => {
                    let value = next() as i64;
                    let result = table.set(&key, &int_patch(&value)).unwrap();
                    if model.contains_key(&key) || model.len() < 128 {
                        assert_eq!(result, Write::Stored);
                        model.entry(key).or_insert((0, 0)).0 = value;
                    } else {
                        assert_eq!(result, Write::Full);
                    }
                }
                2 => assert_eq!(table.delete(&key), model.remove(&key).is_some()),
                3 => {
                    let delta = (next() % 100) as i64 - 50;
                    let result = table.increment_int(&key, 8, delta).unwrap();
                    if model.contains_key(&key) || model.len() < 128 {
                        let entry = model.entry(key).or_insert((0, 0));
                        entry.1 += delta;
                        assert_eq!(result, Some(entry.1));
                    } else {
                        assert_eq!(result, None);
                    }
                }
                _ => {
                    let mut out = [0_u8; 16];
                    let found = table.get(&key, &mut out);
                    match model.get(&key) {
                        Some((first, second)) => {
                            assert!(found);
                            assert_eq!(i64::from_ne_bytes(out[..8].try_into().unwrap()), *first);
                            assert_eq!(i64::from_ne_bytes(out[8..].try_into().unwrap()), *second);
                        }
                        None => assert!(!found),
                    }
                }
            }
            assert_eq!(table.count(), model.len() as u64);
        }
    }

    #[test]
    fn threads_increment_without_losing_updates() {
        let table = Arc::new(table(64, 8));
        let threads: Vec<_> = (0..8)
            .map(|thread| {
                let table = table.clone();
                std::thread::spawn(move || {
                    for index in 0..20_000 {
                        table
                            .increment_int(format!("c{}", index % 16).as_bytes(), 0, 1)
                            .unwrap();
                        let key = format!("t{thread}-{}", index % 4);
                        let value = index as i64;
                        table.set(key.as_bytes(), &int_patch(&value)).unwrap();
                        assert!(read_int(&table, key.as_bytes()).is_some());
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let total: i64 = (0..16)
            .map(|key| read_int(&table, format!("c{key}").as_bytes()).unwrap())
            .sum();
        assert_eq!(total, 8 * 20_000);
    }

    #[cfg(target_os = "linux")]
    fn fork_children(children: usize, work: impl Fn(usize)) {
        let mut pids = Vec::new();
        for child in 0..children {
            // SAFETY: the child only touches shared memory and exits.
            let pid = unsafe { libc::fork() };
            assert!(pid >= 0);
            if pid == 0 {
                work(child);
                unsafe { libc::_exit(0) };
            }
            pids.push(pid);
        }
        for pid in pids {
            let mut status = 0;
            unsafe { libc::waitpid(pid, &mut status, 0) };
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "child failed: {status}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn processes_share_rows_and_counters() {
        let table = table(256, 8);
        fork_children(6, |child| {
            for index in 0..10_000 {
                table.increment_int(b"total", 0, 1).unwrap();
                let key = [b'p', child as u8, (index % 20) as u8];
                let value = index as i64;
                table.set(&key, &int_patch(&value)).unwrap();
            }
        });
        assert_eq!(read_int(&table, b"total"), Some(60_000));
        assert_eq!(table.count(), 1 + 6 * 20);
    }

    /// A process killed while it holds a stripe (or in the middle of a row
    /// write) must not wedge or corrupt the table.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_writer_killed_while_holding_a_stripe_is_recovered() {
        let table = table(16, 16);
        table.set(b"keep", &int_patch(&1)).unwrap();
        let hash = table.hash(b"victim");
        let bucket = (hash & u64::from(table.layout.capacity - 1)) as u32;
        let stripe = bucket & (table.layout.stripes - 1);
        // Simulate a writer that died after allocating and linking a row
        // whose write was interrupted, and after leaking another row.
        let (row, leaked) = (table.allocate().unwrap(), table.allocate().unwrap());
        {
            let header = table.row_header(row);
            header.hash.store(hash, Ordering::Relaxed);
            header.key_length.store(6, Ordering::Relaxed);
            unsafe { ptr::copy_nonoverlapping(b"victim".as_ptr(), table.key_ptr(row), 6) };
            header.next.store(
                table.bucket(bucket).load(Ordering::Relaxed),
                Ordering::Relaxed,
            );
            header.state.store(STATE_WRITING, Ordering::Relaxed);
            table.bucket(bucket).store(row + 1, Ordering::Relaxed);
            table.stripe(stripe).count.fetch_add(1, Ordering::Relaxed);
        }
        let _ = leaked;
        // SAFETY: the child exits immediately; its PID becomes the dead owner.
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            unsafe { libc::_exit(0) };
        }
        let mut status = 0;
        unsafe { libc::waitpid(pid, &mut status, 0) };
        table
            .stripe(stripe)
            .lock
            .store(pid as u32, Ordering::Release);

        assert!(
            !table.exists(b"victim"),
            "an interrupted write must never be visible"
        );
        assert_eq!(table.recoveries(), 1);
        assert_eq!(read_int(&table, b"keep"), Some(1));
        assert_eq!(table.count(), 1);
        // Every row, including the two lost ones, is usable again.
        for index in 0..15_i64 {
            assert_eq!(
                table
                    .set(format!("n{index}").as_bytes(), &int_patch(&index))
                    .unwrap(),
                Write::Stored,
                "row {index}"
            );
        }
        assert_eq!(table.count(), 16);
        assert_eq!(table.set(b"one-too-many", &[]).unwrap(), Write::Full);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn sigkill_storm_never_corrupts_or_wedges_the_table() {
        let table = table(512, 64);
        for round in 0..40 {
            let mut pids = Vec::new();
            for child in 0..4 {
                let pid = unsafe { libc::fork() };
                if pid == 0 {
                    // No allocation in the forked child of a threaded test.
                    let mut index = 0_u64;
                    loop {
                        let slot = (index % 300) as u16;
                        let key = [b'r', child as u8, (slot >> 8) as u8, slot as u8];
                        let value = [((index % 251) as u8); 64];
                        let _ = table.set(
                            &key,
                            &[Patch {
                                offset: 0,
                                bytes: &value,
                            }],
                        );
                        let _ = table.increment_int(b"hits", 0, 1);
                        if index % 3 == 0 {
                            table.delete(&key);
                        }
                        index += 1;
                    }
                }
                pids.push(pid);
            }
            std::thread::sleep(Duration::from_millis(5 + round % 7));
            for pid in pids {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                    libc::waitpid(pid, ptr::null_mut(), 0);
                }
            }
            // Every visible row is consistent: all 64 bytes equal.
            let mut cursor = 0;
            let mut buffer = Vec::new();
            let rows = table.scan(&mut cursor, usize::MAX, &mut buffer);
            let mut offset = 0;
            for _ in 0..rows {
                let length =
                    u32::from_ne_bytes(buffer[offset..offset + 4].try_into().unwrap()) as usize;
                let key = &buffer[offset + 4..offset + 4 + length];
                let value = &buffer[offset + 4 + length..offset + 4 + length + 64];
                if key != b"hits" {
                    assert!(
                        value.iter().all(|byte| *byte == value[0]),
                        "torn row {key:?}"
                    );
                }
                offset += 4 + length + 64;
            }
            assert_eq!(rows as u64, table.count());
        }
        // The table is still fully usable.
        table.collect();
        let live = table.count();
        let mut stored = 0;
        while table.set(format!("fill-{stored}").as_bytes(), &[]).unwrap() == Write::Stored {
            stored += 1;
        }
        assert_eq!(live + stored, 512);
    }

    #[test]
    fn named_tables_share_one_segment_within_a_process() {
        let layout = Layout::new(8, 16, 8, 3).unwrap();
        let first = open("unit.shared", layout).unwrap();
        let second = open("unit.shared", layout).unwrap();
        first.set(b"x", &int_patch(&9)).unwrap();
        assert_eq!(read_int(&second, b"x"), Some(9));
        let other = open("unit.shared", Layout::new(8, 16, 8, 4).unwrap()).unwrap();
        assert!(
            !other.exists(b"x"),
            "a different schema gets its own segment"
        );
        assert!(open("bad name", layout).is_err());
    }

    #[test]
    fn hashing_is_seeded() {
        let mut first = SeededHasher::new(1);
        first.write(b"same");
        let mut second = SeededHasher::new(2);
        second.write(b"same");
        assert_ne!(first.finish(), second.finish());
    }
}
