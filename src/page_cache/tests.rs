//! Store-level tests of the edge page cache, including a randomized model
//! check proving a response is only ever served to requests it is valid for.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, Bytes, to_bytes};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};

use super::policy::{Purge, RequestRules, ResponseRules, UrlPattern};
use super::*;

fn config() -> PageCacheConfig {
    PageCacheConfig {
        requests: RequestRules {
            header_driven: true,
            paths: Vec::new(),
            allow_cookies: false,
            allow_authorization: false,
            ignored_query_params: vec!["utm_*".to_owned()],
            vary_headers: Vec::new(),
        },
        responses: ResponseRules {
            statuses: vec![200, 203, 204, 300, 301, 308, 404, 410],
            default_ttl: Duration::from_secs(30),
            default_stale_while_revalidate: Duration::ZERO,
            max_ttl: Duration::from_secs(86_400),
        },
        max_entries: 1_024,
        max_bytes: 4 * 1024 * 1024,
        max_object_bytes: 1024 * 1024,
        compress: false,
        tag_headers: vec![HeaderName::from_static("pam-cache-tags")],
        invalidation_log: None,
        status_header: true,
    }
}

fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for (name, value) in pairs {
        map.append(
            HeaderName::try_from(*name).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    map
}

fn key(cache: &PageCache, path: &str, query: Option<&str>, request: &HeaderMap) -> RequestKey {
    cache
        .request_key(
            &Method::GET,
            "http",
            Some("example.com"),
            path,
            query,
            request,
        )
        .unwrap()
}

fn store(
    cache: &Arc<PageCache>,
    key: &RequestKey,
    request: &HeaderMap,
    response: &HeaderMap,
    body: &str,
) -> Option<Arc<Entry>> {
    let mut response = response.clone();
    let tags = cache.take_tags(&mut response);
    let policy = cache.store_policy(key, StatusCode::OK, &response).ok()?;
    cache.store(
        key,
        request,
        StatusCode::OK,
        &response,
        Bytes::from(body.to_owned()),
        policy,
        tags,
        cache.epoch(),
    )
}

async fn body_of(response: Response) -> Bytes {
    to_bytes(response.into_body(), usize::MAX).await.unwrap()
}

fn hit_body(cache: &PageCache, key: &RequestKey, request: &HeaderMap) -> Option<Bytes> {
    match cache.lookup(key, request) {
        Lookup::Hit(entry, _) => Some(entry.body.clone()),
        Lookup::Miss => None,
    }
}

#[test]
fn stores_and_serves_public_responses() {
    let cache = PageCache::new(config());
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    assert!(matches!(cache.lookup(&key, &request), Lookup::Miss));
    store(
        &cache,
        &key,
        &request,
        &headers(&[("cache-control", "public, s-maxage=60")]),
        "hello",
    )
    .unwrap();
    assert_eq!(hit_body(&cache, &key, &request).unwrap(), "hello");
    cache.check_invariants();
}

#[test]
fn response_vary_partitions_variants() {
    let cache = PageCache::new(config());
    let english = headers(&[("accept-language", "en")]);
    let portuguese = headers(&[("accept-language", "pt")]);
    let none = HeaderMap::new();
    let response = headers(&[
        ("cache-control", "s-maxage=60"),
        ("vary", "Accept-Language"),
    ]);
    let english_key = key(&cache, "/page", None, &english);
    store(&cache, &english_key, &english, &response, "en").unwrap();
    store(
        &cache,
        &key(&cache, "/page", None, &portuguese),
        &portuguese,
        &response,
        "pt",
    )
    .unwrap();
    assert_eq!(hit_body(&cache, &english_key, &english).unwrap(), "en");
    assert_eq!(hit_body(&cache, &english_key, &portuguese).unwrap(), "pt");
    assert!(hit_body(&cache, &english_key, &none).is_none());
    let mixed = headers(&[("accept-language", "en"), ("accept-language", "pt")]);
    assert!(hit_body(&cache, &english_key, &mixed).is_none());
    cache.check_invariants();
}

#[test]
fn vary_on_cookie_never_shares_between_sessions() {
    let mut config = config();
    config.requests.allow_cookies = true;
    let cache = PageCache::new(config);
    let alice = headers(&[("cookie", "session=alice")]);
    let bob = headers(&[("cookie", "session=bob")]);
    let response = headers(&[("cache-control", "s-maxage=60"), ("vary", "Cookie")]);
    let alice_key = key(&cache, "/page", None, &alice);
    store(&cache, &alice_key, &alice, &response, "alice").unwrap();
    assert_eq!(key(&cache, "/page", None, &bob).primary, alice_key.primary);
    assert!(hit_body(&cache, &alice_key, &bob).is_none());
    assert!(hit_body(&cache, &alice_key, &HeaderMap::new()).is_none());
    assert_eq!(hit_body(&cache, &alice_key, &alice).unwrap(), "alice");
}

#[test]
fn credentials_disable_the_cache_by_default() {
    let cache = PageCache::new(config());
    for request in [
        headers(&[("cookie", "session=alice")]),
        headers(&[("authorization", "Bearer alice")]),
        headers(&[("Cookie", "")]),
    ] {
        assert!(
            cache
                .request_key(
                    &Method::GET,
                    "http",
                    Some("example.com"),
                    "/page",
                    None,
                    &request
                )
                .is_err()
        );
    }
}

#[test]
fn uncacheable_answers_evict_and_stop_collapsing() {
    let cache = PageCache::new(config());
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    store(
        &cache,
        &key,
        &request,
        &headers(&[("cache-control", "s-maxage=60")]),
        "public",
    )
    .unwrap();
    for refused in [
        headers(&[("cache-control", "private, max-age=60")]),
        headers(&[("cache-control", "s-maxage=60"), ("set-cookie", "a=b")]),
        headers(&[("cache-control", "no-store")]),
        headers(&[("cache-control", "s-maxage=60"), ("vary", "*")]),
    ] {
        assert!(store(&cache, &key, &request, &refused, "x").is_none());
    }
    cache.reject(&key, &request);
    assert!(hit_body(&cache, &key, &request).is_none());
    assert!(cache.recently_uncacheable(&key.primary));
    store(
        &cache,
        &key,
        &request,
        &headers(&[("cache-control", "s-maxage=60")]),
        "again",
    )
    .unwrap();
    assert!(!cache.recently_uncacheable(&key.primary));
    cache.check_invariants();
}

#[test]
fn purges_by_tag_url_and_all() {
    let cache = PageCache::new(config());
    let request = HeaderMap::new();
    let tagged =
        |tags: &str| headers(&[("cache-control", "s-maxage=60"), ("pam-cache-tags", tags)]);
    let products = key(&cache, "/products", None, &request);
    let product = key(&cache, "/products/42", Some("a=1"), &request);
    let home = key(&cache, "/", None, &request);
    store(&cache, &products, &request, &tagged("products"), "list").unwrap();
    store(
        &cache,
        &product,
        &request,
        &tagged("products, product:42"),
        "one",
    )
    .unwrap();
    store(&cache, &home, &request, &tagged("home"), "home").unwrap();

    assert_eq!(cache.purge(&[Purge::Tag("product:42".to_owned())]), 1);
    assert!(hit_body(&cache, &product, &request).is_none());
    assert!(hit_body(&cache, &products, &request).is_some());

    store(&cache, &product, &request, &tagged("product:42"), "one").unwrap();
    let pattern = UrlPattern::parse("/products/42?a=2", &[]).unwrap();
    assert_eq!(cache.purge(&[Purge::Url(pattern)]), 0);
    let pattern = UrlPattern::parse("/products/*", &[]).unwrap();
    assert_eq!(cache.purge(&[Purge::Url(pattern)]), 1);
    let pattern = UrlPattern::parse("http://other.test/products", &[]).unwrap();
    assert_eq!(cache.purge(&[Purge::Url(pattern)]), 0);
    let pattern = UrlPattern::parse("http://example.com/products", &[]).unwrap();
    assert_eq!(cache.purge(&[Purge::Url(pattern)]), 1);
    assert_eq!(cache.purge(&[Purge::All]), 1);
    assert_eq!(cache.stats().entries, 0);
    assert_eq!(cache.stats().bytes, 0);
    cache.check_invariants();
}

#[test]
fn a_purge_during_a_fetch_prevents_storing_its_response() {
    let cache = PageCache::new(config());
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    let epoch = cache.epoch();
    cache.purge(&[Purge::Tag("unrelated".to_owned())]);
    let response = headers(&[("cache-control", "s-maxage=60")]);
    let policy = cache.store_policy(&key, StatusCode::OK, &response).unwrap();
    assert!(
        cache
            .store(
                &key,
                &request,
                StatusCode::OK,
                &response,
                Bytes::from_static(b"old"),
                policy,
                Vec::new(),
                epoch
            )
            .is_none()
    );
}

#[test]
fn lru_respects_byte_and_entry_bounds() {
    let mut config = config();
    config.max_bytes = 64 * 1024;
    config.max_entries = 40;
    let cache = PageCache::new(config);
    let request = HeaderMap::new();
    let response = headers(&[("cache-control", "s-maxage=60")]);
    let body = "x".repeat(3_000);
    let hot = key(&cache, "/hot", None, &request);
    store(&cache, &hot, &request, &response, &body).unwrap();
    for index in 0..5_000 {
        let cold = key(&cache, &format!("/cold/{index}"), None, &request);
        store(&cache, &cold, &request, &response, &body).unwrap();
        // Keep one entry hot; LRU must never evict it.
        assert!(
            hit_body(&cache, &hot, &request).is_some(),
            "hot entry evicted at {index}"
        );
        if index % 97 == 0 {
            cache.check_invariants();
        }
    }
    let stats = cache.stats();
    assert!(stats.bytes <= 64 * 1024);
    assert!(stats.entries <= 40);
    assert!(stats.evictions > 4_000);
    cache.check_invariants();
}

#[test]
fn oversized_objects_are_not_stored() {
    let mut config = config();
    config.max_object_bytes = 1_000;
    let cache = PageCache::new(config);
    let request = HeaderMap::new();
    let key = key(&cache, "/big", None, &request);
    assert!(
        store(
            &cache,
            &key,
            &request,
            &headers(&[("cache-control", "s-maxage=60")]),
            &"x".repeat(1_001)
        )
        .is_none()
    );
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn variants_per_url_are_bounded() {
    let cache = PageCache::new(config());
    let response = headers(&[("cache-control", "s-maxage=60"), ("vary", "x-variant")]);
    for index in 0..200 {
        let request = headers(&[("x-variant", &index.to_string())]);
        let key = key(&cache, "/page", None, &request);
        store(&cache, &key, &request, &response, &index.to_string()).unwrap();
    }
    assert_eq!(cache.stats().entries as usize, MAX_VARIANTS);
    cache.check_invariants();
}

#[test]
fn expired_entries_are_misses_and_stale_ones_are_flagged() {
    let mut config = config();
    config.responses.max_ttl = Duration::from_millis(30);
    let cache = PageCache::new(config);
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    store(
        &cache,
        &key,
        &request,
        &headers(&[("cache-control", "s-maxage=60, stale-while-revalidate=60")]),
        "x",
    )
    .unwrap();
    assert!(matches!(
        cache.lookup(&key, &request),
        Lookup::Hit(_, HitKind::Fresh)
    ));
    std::thread::sleep(Duration::from_millis(40));
    assert!(matches!(
        cache.lookup(&key, &request),
        Lookup::Hit(_, HitKind::Stale)
    ));
    std::thread::sleep(Duration::from_millis(30));
    assert!(matches!(cache.lookup(&key, &request), Lookup::Miss));
    assert_eq!(
        cache.stats().entries,
        0,
        "expired entries are removed on lookup"
    );
    cache.check_invariants();
}

#[tokio::test]
async fn conditional_head_and_encoded_responses() {
    let mut config = config();
    config.compress = true;
    let cache = PageCache::new(config);
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    let body = "<p>hello world</p>".repeat(200);
    let entry = store(
        &cache,
        &key,
        &request,
        &headers(&[
            ("cache-control", "s-maxage=60"),
            ("content-type", "text/html; charset=utf-8"),
        ]),
        &body,
    )
    .unwrap();
    for _ in 0..200 {
        if entry.encoded.get().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(entry.encoded.get().is_some(), "compression finished");
    cache.check_invariants();

    let (response, length) = cache.respond(&entry, "HIT", &request, false);
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(length, body.len());
    let etag = response.headers()[http::header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(response.headers()["x-pam-cache"], "HIT");
    assert!(
        response.headers()[http::header::VARY]
            .to_str()
            .unwrap()
            .contains("Accept-Encoding")
    );
    assert_eq!(body_of(response).await, body);

    let (response, _) = cache.respond(
        &entry,
        "HIT",
        &headers(&[("accept-encoding", "gzip, br")]),
        false,
    );
    assert_eq!(response.headers()[http::header::CONTENT_ENCODING], "br");
    let br_etag = response.headers()[http::header::ETAG]
        .to_str()
        .unwrap()
        .to_owned();
    assert_ne!(br_etag, etag);
    let compressed = body_of(response).await;
    let mut decoded = Vec::new();
    brotli::BrotliDecompress(&mut &compressed[..], &mut decoded).unwrap();
    assert_eq!(decoded, body.as_bytes());

    let (response, _) = cache.respond(
        &entry,
        "HIT",
        &headers(&[("accept-encoding", "gzip")]),
        false,
    );
    assert_eq!(response.headers()[http::header::CONTENT_ENCODING], "gzip");
    let compressed = body_of(response).await;
    let mut decoded = String::new();
    std::io::Read::read_to_string(
        &mut flate2::read::GzDecoder::new(&compressed[..]),
        &mut decoded,
    )
    .unwrap();
    assert_eq!(decoded, body);

    for validator in [etag.as_str(), br_etag.as_str(), &format!("W/{etag}")] {
        let (response, length) = cache.respond(
            &entry,
            "HIT",
            &headers(&[("if-none-match", validator)]),
            false,
        );
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{validator}");
        assert_eq!(length, 0);
        assert!(response.headers().contains_key(http::header::ETAG));
        assert!(!response.headers().contains_key(http::header::CONTENT_TYPE));
        assert!(body_of(response).await.is_empty());
    }
    let (response, _) = cache.respond(
        &entry,
        "HIT",
        &headers(&[("if-none-match", "\"other\"")]),
        false,
    );
    assert_eq!(response.status(), StatusCode::OK);

    let (response, length) = cache.respond(&entry, "HIT", &request, true);
    assert_eq!(length, 0);
    assert_eq!(
        response.headers()[http::header::CONTENT_LENGTH],
        body.len().to_string()
    );
    assert!(body_of(response).await.is_empty());
    assert_eq!(cache.stats().not_modified, 3);
}

#[test]
fn applications_etags_are_kept() {
    let cache = PageCache::new(config());
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    let entry = store(
        &cache,
        &key,
        &request,
        &headers(&[("cache-control", "s-maxage=60"), ("etag", "\"v1\"")]),
        "x",
    )
    .unwrap();
    let (response, _) = cache.respond(
        &entry,
        "HIT",
        &headers(&[("if-none-match", "\"v1\"")]),
        false,
    );
    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
}

#[test]
fn stored_headers_exclude_hop_by_hop_and_tag_fields() {
    let cache = PageCache::new(config());
    let request = HeaderMap::new();
    let key = key(&cache, "/page", None, &request);
    let entry = store(
        &cache,
        &key,
        &request,
        &headers(&[
            ("cache-control", "s-maxage=60"),
            ("connection", "close"),
            ("date", "Tue, 10 Nov 2026 10:00:00 GMT"),
            ("pam-cache-tags", "a"),
            ("x-custom", "kept"),
        ]),
        "x",
    )
    .unwrap();
    let (response, _) = cache.respond(&entry, "HIT", &request, false);
    let response = response.headers();
    assert!(!response.contains_key("connection"));
    assert!(!response.contains_key("date"));
    assert!(!response.contains_key("pam-cache-tags"));
    assert_eq!(response["x-custom"], "kept");
    assert_eq!(response[http::header::AGE], "0");
}

#[tokio::test]
async fn one_leader_per_url_and_followers_are_released() {
    let cache = PageCache::new(config());
    let leader = match cache.begin_flight("k") {
        Flight::Leader(guard) => guard,
        Flight::Follower(_) => panic!("first flight leads"),
    };
    let mut followers = Vec::new();
    for _ in 0..8 {
        match cache.begin_flight("k") {
            Flight::Follower(receiver) => followers.push(receiver),
            Flight::Leader(_) => panic!("only one leader"),
        }
    }
    assert!(matches!(cache.begin_flight("other"), Flight::Leader(_)));
    let waiters: Vec<_> = followers
        .into_iter()
        .map(|mut receiver| tokio::spawn(async move { receiver.changed().await.is_err() }))
        .collect();
    tokio::time::sleep(Duration::from_millis(10)).await;
    drop(leader);
    for waiter in waiters {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), waiter)
                .await
                .unwrap()
                .unwrap()
        );
    }
    assert_eq!(cache.flights_in_progress(), 0, "flights never leak");
    assert!(matches!(cache.begin_flight("k"), Flight::Leader(_)));
}

#[tokio::test]
async fn collect_limited_replays_oversized_bodies() {
    let body = Body::from("abcdef");
    assert_eq!(collect_limited(body, 6).await.unwrap(), "abcdef");
    let chunks = futures_util::stream::iter(
        ["ab", "cd", "ef", "gh"]
            .map(|chunk| Ok::<_, std::io::Error>(Bytes::from_static(chunk.as_bytes()))),
    );
    let replayed = collect_limited(Body::from_stream(chunks), 3)
        .await
        .unwrap_err();
    assert_eq!(to_bytes(replayed, usize::MAX).await.unwrap(), "abcdefgh");
}

#[test]
fn invalidation_log_round_trip_rotation_and_truncation() {
    let directory = std::env::temp_dir().join(format!("pam-page-cache-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let path = directory.join("nested/log");
    let mut config = config();
    config.invalidation_log = Some(path.clone());
    let reader = PageCache::new(config.clone());
    let request = HeaderMap::new();
    let tagged = headers(&[
        ("cache-control", "s-maxage=60"),
        ("pam-cache-tags", "products"),
    ]);
    let fill = |cache: &Arc<PageCache>| {
        for index in 0..3 {
            let key = key(cache, &format!("/p/{index}"), None, &request);
            store(cache, &key, &request, &tagged, "x").unwrap();
        }
        let key = key(cache, "/other", None, &request);
        store(
            cache,
            &key,
            &request,
            &headers(&[("cache-control", "s-maxage=60")]),
            "x",
        )
        .unwrap();
    };
    fill(&reader);
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 4);

    let writer = PageCache::new(config.clone());
    writer
        .purge_everywhere(&[Purge::Tag("products".to_owned())])
        .unwrap();
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 1, "tag purge reached the sibling");

    publish_invalidations(
        &path,
        &[Purge::Url(UrlPattern::parse("/other", &[]).unwrap())],
    )
    .unwrap();
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 0, "URL purge reached the sibling");

    // A partially written line waits for its newline.
    fill(&reader);
    {
        use std::io::Write as _;
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"prod").unwrap();
        reader.synchronize(true);
        assert_eq!(reader.stats().entries, 4);
        file.write_all(b"ucts\n").unwrap();
    }
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 1);

    // Truncation and rotation purge everything: lines may have been lost.
    fill(&reader);
    std::fs::write(&path, b"").unwrap();
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 0, "truncated log purges all");
    fill(&reader);
    let big = vec![Purge::Tag("t".repeat(100)); 12_000];
    publish_invalidations(&path, &big).unwrap();
    assert!(std::fs::metadata(&path).unwrap().len() < 1024);
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 0, "rotated log purges all");

    // Invalid lines are ignored.
    fill(&reader);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .and_then(|mut file| std::io::Write::write_all(&mut file, b"bad tag!\n!url relative\n"))
        .unwrap();
    reader.synchronize(true);
    assert_eq!(reader.stats().entries, 4);
    let _ = std::fs::remove_dir_all(&directory);
}

// ---------------------------------------------------------------------------
// Randomized model check: a response is never served to a request it is not
// valid for, whatever mix of credentials, Vary, Cache-Control, queries,
// forwarded headers, purges and evictions.
// ---------------------------------------------------------------------------

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        // xorshift64*: deterministic, reproducible from the seed.
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

const QUERY_PAIRS: [&str; 11] = [
    "a=1",
    "b=2",
    "a=3",
    "a.b=4",
    "a_b=5",
    "x[]=1",
    "x[]=2",
    "%61=9",
    "utm_source=z",
    "c=",
    "page=2",
];

/// What PHP's `$_GET` would contain (minus ignored parameters).
fn php_query(query: &str) -> BTreeMap<String, Vec<String>> {
    fn decode(value: &str) -> String {
        let bytes = value.as_bytes();
        let mut output = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'%' && index + 2 < bytes.len() {
                if let Ok(byte) = u8::from_str_radix(&value[index + 1..index + 3], 16) {
                    output.push(byte);
                    index += 3;
                    continue;
                }
            }
            output.push(if bytes[index] == b'+' {
                b' '
            } else {
                bytes[index]
            });
            index += 1;
        }
        String::from_utf8_lossy(&output).into_owned()
    }
    let mut parsed: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = decode(name);
        if name.starts_with("utm_") {
            continue;
        }
        let (base, array) = match name.strip_suffix("[]") {
            Some(base) => (base.to_owned(), true),
            None => (name.clone(), false),
        };
        let base = base.replace(['.', ' '], "_");
        let value = decode(value);
        if array {
            parsed.entry(format!("{base}[]")).or_default().push(value);
        } else {
            parsed.insert(base, vec![value]);
        }
    }
    parsed
}

#[derive(Clone, Debug, PartialEq)]
struct Observed {
    scheme: &'static str,
    host: String,
    path: String,
    query: BTreeMap<String, Vec<String>>,
    forwarded_host: Option<String>,
    cookie: Option<String>,
    authorization: Option<String>,
    tenant: Option<String>,
    language: Option<String>,
}

#[derive(Clone, Copy, Debug)]
enum AppResponse {
    Public,
    PublicVaryTenant,
    PublicVaryLanguageTenant,
    PublicVaryCookie,
    PublicVaryAuthorization,
    PublicSetCookie,
    Private,
    NoStore,
    NoCache,
    Plain,
    VaryStar,
    PublicMaxAgeOnly,
    SharedMaxAgePrivate,
    SharedMaxAgeUppercasePrivate,
}

const APP_RESPONSES: [AppResponse; 14] = [
    AppResponse::Public,
    AppResponse::PublicVaryTenant,
    AppResponse::PublicVaryLanguageTenant,
    AppResponse::PublicVaryCookie,
    AppResponse::PublicVaryAuthorization,
    AppResponse::PublicSetCookie,
    AppResponse::Private,
    AppResponse::NoStore,
    AppResponse::NoCache,
    AppResponse::Plain,
    AppResponse::VaryStar,
    AppResponse::PublicMaxAgeOnly,
    AppResponse::SharedMaxAgePrivate,
    AppResponse::SharedMaxAgeUppercasePrivate,
];

impl AppResponse {
    fn headers(self) -> HeaderMap {
        match self {
            Self::Public => headers(&[
                ("cache-control", "public, s-maxage=60"),
                ("pam-cache-tags", "t1"),
            ]),
            Self::PublicVaryTenant => {
                headers(&[("cache-control", "s-maxage=60"), ("vary", "X-Tenant")])
            }
            Self::PublicVaryLanguageTenant => headers(&[
                ("cache-control", "s-maxage=60"),
                ("vary", "accept-language"),
                ("vary", "x-tenant, accept-encoding"),
            ]),
            Self::PublicVaryCookie => {
                headers(&[("cache-control", "s-maxage=60"), ("vary", "Cookie")])
            }
            Self::PublicVaryAuthorization => {
                headers(&[("cache-control", "s-maxage=60"), ("vary", "Authorization")])
            }
            Self::PublicSetCookie => headers(&[
                ("cache-control", "public, s-maxage=60"),
                ("set-cookie", "session=new"),
            ]),
            Self::Private => headers(&[("cache-control", "private, max-age=60")]),
            Self::NoStore => headers(&[("cache-control", "public, s-maxage=60, no-store")]),
            Self::NoCache => headers(&[("cache-control", "public, s-maxage=60, no-cache")]),
            Self::Plain => HeaderMap::new(),
            Self::VaryStar => headers(&[("cache-control", "s-maxage=60"), ("vary", "*")]),
            Self::PublicMaxAgeOnly => headers(&[("cache-control", "max-age=60")]),
            Self::SharedMaxAgePrivate => headers(&[
                ("cache-control", "public, s-maxage=60"),
                ("cache-control", "private"),
            ]),
            Self::SharedMaxAgeUppercasePrivate => {
                headers(&[("cache-control", "S-MAXAGE=60, PRIVATE")])
            }
        }
    }

    fn storable(self) -> bool {
        matches!(
            self,
            Self::Public
                | Self::PublicVaryTenant
                | Self::PublicVaryLanguageTenant
                | Self::PublicVaryCookie
                | Self::PublicVaryAuthorization
        )
    }

    fn varies_on(self) -> &'static [&'static str] {
        match self {
            Self::PublicVaryTenant => &["tenant"],
            Self::PublicVaryLanguageTenant => &["language", "tenant"],
            Self::PublicVaryCookie => &["cookie"],
            Self::PublicVaryAuthorization => &["authorization"],
            _ => &[],
        }
    }
}

/// Encodes what the "application" saw into the body, so a hit can be checked
/// against the request it is served to.
fn encode(observed: &Observed, response: usize) -> String {
    format!("{response}\u{1}{observed:?}")
}

struct Generated {
    host: String,
    scheme: &'static str,
    path: String,
    query: Option<String>,
    headers: HeaderMap,
    observed: Observed,
}

fn generate(random: &mut Random) -> Generated {
    let host = random
        .pick(&["example.com", "Example.com", "other.test"])
        .to_string();
    let scheme = *random.pick(&["http", "https"]);
    let path = random
        .pick(&["/", "/page", "/page/", "/Page", "/a/../page"])
        .to_string();
    let query = if random.chance(30) {
        None
    } else {
        let count = random.below(4);
        Some(
            (0..count)
                .map(|_| *random.pick(&QUERY_PAIRS))
                .collect::<Vec<_>>()
                .join("&"),
        )
    };
    let mut request = HeaderMap::new();
    let mut maybe = |random: &mut Random, name: &'static str, values: &[&str], percent: usize| {
        if random.chance(percent) {
            let value = random.pick(values).to_string();
            request.append(
                HeaderName::from_static(name),
                HeaderValue::from_str(&value).unwrap(),
            );
            if random.chance(5) {
                let again = random.pick(values).to_string();
                request.append(
                    HeaderName::from_static(name),
                    HeaderValue::from_str(&again).unwrap(),
                );
            }
        }
    };
    maybe(
        random,
        "cookie",
        &[
            "session=alice",
            "session=bob",
            "session=carol; theme=dark",
            "",
        ],
        40,
    );
    maybe(
        random,
        "authorization",
        &["Bearer alice", "Bearer bob", "Basic Y2Fyb2w6eA=="],
        25,
    );
    maybe(random, "x-tenant", &["acme", "globex", "ACME"], 40);
    maybe(random, "accept-language", &["en", "pt-BR", "en, pt"], 40);
    maybe(
        random,
        "x-forwarded-host",
        &["evil.test", "example.com"],
        10,
    );
    maybe(random, "accept-encoding", &["gzip", "br", "identity"], 40);
    let joined = |name: &str| {
        let values: Vec<String> = request
            .get_all(name)
            .iter()
            .map(|value| value.to_str().unwrap().to_owned())
            .collect();
        (!values.is_empty()).then(|| values.join("\n"))
    };
    let observed = Observed {
        scheme,
        host: host.to_ascii_lowercase(),
        path: path.clone(),
        query: php_query(query.as_deref().unwrap_or("")),
        forwarded_host: joined("x-forwarded-host"),
        cookie: joined("cookie"),
        authorization: joined("authorization"),
        tenant: joined("x-tenant"),
        language: joined("accept-language"),
    };
    Generated {
        host,
        scheme,
        path,
        query,
        headers: request,
        observed,
    }
}

fn verify_hit(body: &[u8], request: &Observed, config: &PageCacheConfig) {
    let body = std::str::from_utf8(body).unwrap();
    let (response, stored) = body.split_once('\u{1}').unwrap();
    let response = APP_RESPONSES[response.parse::<usize>().unwrap()];
    assert!(
        response.storable(),
        "served an uncacheable response: {response:?}"
    );
    let served = blank_fields_not_varied(stored, response);
    let expected = blank_fields_not_varied(&format!("{request:?}"), response);
    assert_eq!(
        served, expected,
        "served a response produced for a different request"
    );
    if !config.requests.allow_cookies {
        assert!(request.cookie.is_none(), "cookie request used the cache");
        assert!(!stored.contains("cookie: Some"), "stored a cookie request");
    }
    if !config.requests.allow_authorization {
        assert!(
            request.authorization.is_none(),
            "authorized request used the cache"
        );
        assert!(
            !stored.contains("authorization: Some"),
            "stored an authorized request"
        );
    }
}

fn blank_fields_not_varied(observed: &str, response: AppResponse) -> String {
    let mut result = observed.to_owned();
    for (name, prefix) in [
        ("tenant", "tenant: "),
        ("language", "language: "),
        ("cookie", "cookie: "),
        ("authorization", "authorization: "),
    ] {
        if !response.varies_on().contains(&name) {
            result = blank_field(&result, prefix);
        }
    }
    result
}

/// Replaces `prefix<Option value>` in a Debug string with `prefix_`.
fn blank_field(text: &str, prefix: &str) -> String {
    let Some(start) = text.find(&format!(" {prefix}")) else {
        return text.to_owned();
    };
    let value_start = start + 1 + prefix.len();
    let rest = &text[value_start..];
    let end = if rest.starts_with("None") {
        4
    } else {
        // Some("...") with escaped quotes inside.
        let mut index = 6;
        let bytes = rest.as_bytes();
        while index < bytes.len() {
            if bytes[index] == b'\\' {
                index += 2;
                continue;
            }
            if bytes[index] == b'"' {
                break;
            }
            index += 1;
        }
        index + 2
    };
    format!("{}_{}", &text[..value_start], &rest[end..])
}

fn run_model(
    seed: u64,
    operations: usize,
    config: PageCacheConfig,
    cache: &Arc<PageCache>,
) -> (usize, usize) {
    let mut random = Random(seed | 1);
    let mut hits = 0;
    let mut stores = 0;
    for _ in 0..operations {
        if random.chance(1) {
            let purge = match random.below(3) {
                0 => Purge::All,
                1 => Purge::Tag("t1".to_owned()),
                _ => Purge::Url(
                    UrlPattern::parse("/page", &config.requests.ignored_query_params).unwrap(),
                ),
            };
            cache.purge(&[purge]);
            continue;
        }
        let generated = generate(&mut random);
        let method = if random.chance(10) {
            Method::HEAD
        } else {
            Method::GET
        };
        let Ok(key) = cache.request_key(
            &method,
            generated.scheme,
            Some(&generated.host),
            &generated.path,
            generated.query.as_deref(),
            &generated.headers,
        ) else {
            continue;
        };
        if let Lookup::Hit(entry, _) = cache.lookup(&key, &generated.headers) {
            verify_hit(&entry.body, &generated.observed, &config);
            let (response, _) = cache.respond(&entry, "HIT", &generated.headers, key.head);
            assert!(!response.headers().contains_key(http::header::SET_COOKIE));
            hits += 1;
            continue;
        }
        if key.head {
            continue;
        }
        let index = random.below(APP_RESPONSES.len());
        let app = APP_RESPONSES[index];
        let mut response_headers = app.headers();
        let tags = cache.take_tags(&mut response_headers);
        let epoch = cache.epoch();
        match cache.store_policy(&key, StatusCode::OK, &response_headers) {
            Ok(policy) => {
                assert!(app.storable(), "{app:?} was accepted for storage");
                if cache
                    .store(
                        &key,
                        &generated.headers,
                        StatusCode::OK,
                        &response_headers,
                        Bytes::from(encode(&generated.observed, index)),
                        policy,
                        tags,
                        epoch,
                    )
                    .is_some()
                {
                    stores += 1;
                }
            }
            Err(_) => {
                assert!(!app.storable(), "{app:?} was refused");
                cache.reject(&key, &generated.headers);
            }
        }
    }
    cache.check_invariants();
    (hits, stores)
}

fn model_configs() -> Vec<PageCacheConfig> {
    let mut configs = Vec::new();
    for allow_cookies in [false, true] {
        for allow_authorization in [false, true] {
            for max_bytes in [8 * 1024, 4 * 1024 * 1024] {
                let mut config = config();
                config.requests.allow_cookies = allow_cookies;
                config.requests.allow_authorization = allow_authorization;
                config.max_bytes = max_bytes;
                config.max_entries = 64;
                configs.push(config);
            }
        }
    }
    configs
}

#[test]
fn model_never_serves_a_response_to_a_request_it_was_not_valid_for() {
    let mut total_hits = 0;
    for (index, config) in model_configs().into_iter().enumerate() {
        for seed in 0..4u64 {
            let cache = PageCache::new(config.clone());
            let (hits, stores) = run_model(
                0x9e37_79b9 ^ (seed << 8) ^ index as u64,
                25_000,
                config.clone(),
                &cache,
            );
            assert!(stores > 0, "config {index} stored nothing");
            total_hits += hits;
        }
    }
    assert!(
        total_hits > 10_000,
        "the model exercised too few hits: {total_hits}"
    );
}

#[test]
fn model_holds_under_concurrent_access() {
    for config in model_configs() {
        let cache = PageCache::new(config.clone());
        let threads: Vec<_> = (0..8u64)
            .map(|seed| {
                let cache = cache.clone();
                let config = config.clone();
                std::thread::spawn(move || run_model(0x51_7cc1_b727 ^ seed, 10_000, config, &cache))
            })
            .collect();
        let hits: usize = threads
            .into_iter()
            .map(|thread| thread.join().unwrap().0)
            .sum();
        assert!(hits > 0);
        cache.check_invariants();
    }
}

#[test]
fn blank_field_handles_escaped_values() {
    let observed = Observed {
        scheme: "http",
        host: "h".to_owned(),
        path: "/".to_owned(),
        query: BTreeMap::new(),
        forwarded_host: None,
        cookie: Some("a=\"b\"; c".to_owned()),
        authorization: None,
        tenant: Some("x".to_owned()),
        language: None,
    };
    let text = format!("{observed:?}");
    let blanked = blank_field(&text, "cookie: ");
    assert!(
        blanked.contains("cookie: _, authorization: None"),
        "{blanked}"
    );
}
