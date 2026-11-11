#![no_main]

//! Fuzzes the edge page cache rules: request keys, query normalization,
//! storability, Vary parsing, purge lines and URL patterns. Besides "never
//! panics", it checks the safety properties against an independent oracle.

use libfuzzer_sys::fuzz_target;

#[allow(dead_code)]
#[path = "../../src/page_cache/policy.rs"]
mod policy;

use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};

const NAMES: [&str; 14] = [
    "cookie",
    "authorization",
    "cache-control",
    "pragma",
    "vary",
    "set-cookie",
    "x-forwarded-host",
    "accept-language",
    "range",
    "upgrade",
    "content-range",
    "accept-encoding",
    "if-none-match",
    "x-tenant",
];

fn headers(data: &[u8]) -> HeaderMap {
    let mut map = HeaderMap::new();
    for chunk in data.split(|byte| *byte == 0xff) {
        let Some((selector, value)) = chunk.split_first() else {
            continue;
        };
        let name = HeaderName::from_static(NAMES[*selector as usize % NAMES.len()]);
        if let Ok(value) = HeaderValue::from_bytes(value) {
            map.append(name, value);
        }
    }
    map
}

fn directive_tokens(map: &HeaderMap, name: &str) -> Vec<String> {
    map.get_all(name)
        .iter()
        .map(|value| String::from_utf8_lossy(value.as_bytes()).to_ascii_lowercase())
        .flat_map(|value| {
            value
                .split(',')
                .map(|directive| directive.split('=').next().unwrap_or("").trim().to_owned())
                .collect::<Vec<_>>()
        })
        .collect()
}

fuzz_target!(|data: &[u8]| {
    let mut parts = data.splitn(4, |byte| *byte == 0xfe);
    let target = parts.next().unwrap_or_default();
    let request = headers(parts.next().unwrap_or_default());
    let response = headers(parts.next().unwrap_or_default());
    let line = parts.next().unwrap_or_default();

    let target = String::from_utf8_lossy(target);
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target.as_ref(), None),
    };
    let ignored = vec!["utm_*".to_owned(), "fbclid".to_owned()];
    let rules = policy::RequestRules {
        header_driven: true,
        paths: vec!["/legacy".to_owned()],
        allow_cookies: false,
        allow_authorization: false,
        ignored_query_params: ignored.clone(),
        vary_headers: vec![HeaderName::from_static("accept-language")],
    };
    for method in [Method::GET, Method::HEAD, Method::POST] {
        if let Ok(key) = policy::request_key(&rules, &method, "http", None, path, query, &request) {
            assert!(method != Method::POST);
            assert!(!request.contains_key("cookie"), "cookie request got a key");
            assert!(!request.contains_key("authorization"), "authorized request got a key");
            assert!(!request.contains_key("range"));
            assert!(key.primary.contains(path));
        }
    }

    if let Some(query) = query {
        let normalized = policy::normalize_query(query, &ignored);
        assert_eq!(policy::normalize_query(&normalized, &ignored), normalized, "not idempotent");
        let mut kept: Vec<&str> = query
            .split('&')
            .filter(|pair| !pair.is_empty())
            .filter(|pair| {
                let name = pair.split_once('=').map_or(*pair, |(name, _)| name);
                !name.starts_with("utm_") && name != "fbclid"
            })
            .collect();
        let mut reordered: Vec<&str> = normalized.split('&').filter(|pair| !pair.is_empty()).collect();
        kept.sort_unstable();
        reordered.sort_unstable();
        assert_eq!(kept, reordered, "normalization may only reorder or drop ignored pairs");
    }

    let response_rules = policy::ResponseRules {
        statuses: vec![200, 301, 404],
        default_ttl: Duration::from_secs(1),
        default_stale_while_revalidate: Duration::ZERO,
        max_ttl: Duration::from_secs(60),
    };
    for forced in [false, true] {
        let status = StatusCode::from_u16(200 + (data.len() % 3) as u16 * 101).unwrap_or(StatusCode::OK);
        if let Ok(stored) = policy::store_policy(&response_rules, forced, status, &response) {
            let tokens = directive_tokens(&response, "cache-control");
            for forbidden in ["private", "no-store", "no-cache"] {
                assert!(!tokens.iter().any(|token| token == forbidden), "stored a {forbidden} response");
            }
            assert!(!response.contains_key("set-cookie"), "stored a Set-Cookie response");
            assert!(!response.contains_key("content-range"));
            assert!(!directive_tokens(&response, "vary").iter().any(|token| token == "*"));
            assert!(!directive_tokens(&response, "pragma").iter().any(|token| token == "no-cache"));
            assert!(!stored.ttl.is_zero() && stored.ttl <= Duration::from_secs(60));
        }
    }
    if let Some(names) = policy::vary_names(&response, true) {
        assert!(names.len() <= policy::MAX_VARY_HEADERS);
        let _ = policy::vary_values(&request, &names);
    }
    let _ = policy::negotiate_encoding(&request, true, true);
    let _ = policy::if_none_match_matches(&request, &["\"a\""]);
    let _ = policy::parse_tags(response.get_all("vary").iter());

    let line = String::from_utf8_lossy(line);
    if let Some(purge) = policy::Purge::parse_line(&line, &ignored) {
        let round_trip = policy::Purge::parse_line(&purge.to_line(), &ignored);
        assert_eq!(round_trip, Some(purge), "purge line does not round-trip");
    }
    if let Some(pattern) = policy::UrlPattern::parse(&line, &ignored) {
        let _ = pattern.matches("example.com", path, query.unwrap_or(""));
    }
});
