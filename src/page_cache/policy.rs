//! Pure rules of the edge page cache: which requests may be looked up, how
//! the cache key is built, which responses may be stored, how variants are
//! matched (`Vary`) and how conditional requests and encodings are answered.
//!
//! Everything here is a pure function of its inputs so the rules can be unit
//! tested, property tested and fuzzed without a server (see
//! `fuzz/fuzz_targets/page_cache_policy.rs`). The one rule that must never
//! break: a response produced for one user is never served to another. The
//! request side refuses credentials (`Cookie`, `Authorization`) unless the
//! operator explicitly allows them, the response side refuses `Set-Cookie`,
//! `private`, `no-store`, `no-cache` and `Vary: *`, and every request header a
//! response varies on is matched byte for byte.

use std::time::Duration;

use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode};

/// Request headers that change what a framework renders (absolute URLs,
/// scheme, base path) without being part of the URL. Keying on them prevents
/// the classic unkeyed-header cache poisoning (`X-Forwarded-Host: evil`).
pub const UNKEYED_INPUT_HEADERS: [&str; 8] = [
    "forwarded",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-forwarded-port",
    "x-forwarded-prefix",
    "x-forwarded-scheme",
    "x-original-url",
    "x-rewrite-url",
];

/// Upper bound on the `Vary` header names one stored variant may carry.
pub const MAX_VARY_HEADERS: usize = 16;
/// Upper bound on cache tags kept per response.
pub const MAX_TAGS: usize = 64;
/// Upper bound on the length of one cache tag.
pub const MAX_TAG_BYTES: usize = 128;

#[derive(Clone, Debug)]
pub struct RequestRules {
    /// Header-driven mode: any GET/HEAD may be stored when the response opts in.
    pub header_driven: bool,
    /// Legacy explicit paths cached with `default_ttl` even without headers.
    pub paths: Vec<String>,
    pub allow_cookies: bool,
    pub allow_authorization: bool,
    /// Query parameters dropped from the key: exact names or `prefix*`.
    pub ignored_query_params: Vec<String>,
    /// Request headers always part of the key (lower-case names).
    pub vary_headers: Vec<HeaderName>,
}

/// Why a request does not use the cache. Exposed for metrics and tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bypass {
    Disabled,
    Method,
    Cookie,
    Authorization,
    Range,
    Upgrade,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestKey {
    /// Primary key: scheme, host, path, normalized query and keyed headers.
    pub primary: String,
    pub host: String,
    pub path: String,
    pub query: String,
    pub head: bool,
    /// The client asked to skip stored copies (`no-cache`): look-up is
    /// skipped but the fresh response may still be stored.
    pub refresh: bool,
    /// The path is one of the legacy explicit `responseCachePaths`.
    pub forced_path: bool,
}

/// Decides whether a request may use the cache and computes its key.
pub fn request_key(
    rules: &RequestRules,
    method: &Method,
    scheme: &str,
    authority: Option<&str>,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
) -> Result<RequestKey, Bypass> {
    let head = *method == Method::HEAD;
    if *method != Method::GET && !head {
        return Err(Bypass::Method);
    }
    let forced_path = rules.paths.iter().any(|configured| configured == path);
    if !rules.header_driven && !forced_path {
        return Err(Bypass::Disabled);
    }
    if headers.contains_key(http::header::AUTHORIZATION) && !rules.allow_authorization {
        return Err(Bypass::Authorization);
    }
    if headers.contains_key(http::header::COOKIE) && !rules.allow_cookies {
        return Err(Bypass::Cookie);
    }
    if headers.contains_key(http::header::RANGE) || headers.contains_key(http::header::IF_RANGE) {
        return Err(Bypass::Range);
    }
    if headers.contains_key(http::header::UPGRADE) {
        return Err(Bypass::Upgrade);
    }
    let refresh = header_directives(headers, http::header::CACHE_CONTROL).any(|(name, _)| {
        name.eq_ignore_ascii_case("no-cache") || name.eq_ignore_ascii_case("no-store")
    }) || header_directives(headers, http::header::PRAGMA)
        .any(|(name, _)| name.eq_ignore_ascii_case("no-cache"));

    let host = authority
        .map(str::to_owned)
        .or_else(|| {
            headers
                .get(http::header::HOST)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        })
        .unwrap_or_default()
        .to_ascii_lowercase();
    let query = normalize_query(query.unwrap_or(""), &rules.ignored_query_params);

    let mut primary =
        String::with_capacity(scheme.len() + host.len() + path.len() + query.len() + 8);
    push_field(&mut primary, scheme);
    push_field(&mut primary, &host);
    push_field(&mut primary, path);
    push_field(&mut primary, &query);
    for name in rules
        .vary_headers
        .iter()
        .map(HeaderName::as_str)
        .chain(UNKEYED_INPUT_HEADERS)
    {
        let mut values = headers.get_all(name).iter().peekable();
        if values.peek().is_none() {
            continue;
        }
        push_field(&mut primary, name);
        for value in values {
            push_field(&mut primary, &String::from_utf8_lossy(value.as_bytes()));
        }
    }
    Ok(RequestKey {
        primary,
        host,
        path: path.to_owned(),
        query,
        head,
        refresh,
        forced_path,
    })
}

/// Length-prefixed fields keep the key unambiguous whatever bytes they hold.
fn push_field(key: &mut String, field: &str) {
    use std::fmt::Write as _;
    let _ = write!(key, "{}:", field.len());
    key.push_str(field);
}

/// Normalizes a query string for the key. Empty pairs and ignored parameters
/// are dropped. Pairs are sorted by name only when every name is a plain
/// `[A-Za-z0-9_-]` token and names are unique: then PHP's `$_GET` cannot
/// depend on their order. Anything else (arrays, repeated names, encoded or
/// dotted names) keeps its original order, so two queries share a key only
/// when PHP parses them to the same values.
pub fn normalize_query(query: &str, ignored: &[String]) -> String {
    let mut pairs: Vec<&str> = query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter(|pair| {
            let name = pair.split_once('=').map_or(*pair, |(name, _)| name);
            !ignored
                .iter()
                .any(|pattern| match pattern.strip_suffix('*') {
                    Some(prefix) => name.starts_with(prefix),
                    None => name == pattern,
                })
        })
        .collect();
    let names: Vec<&str> = pairs
        .iter()
        .map(|pair| pair.split_once('=').map_or(*pair, |(name, _)| name))
        .collect();
    let simple = names.iter().all(|name| {
        !name.is_empty()
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    });
    let unique = {
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.windows(2).all(|window| window[0] != window[1])
    };
    if simple && unique {
        pairs.sort_by(|left, right| {
            let left = left.split_once('=').map_or(*left, |(name, _)| name);
            let right = right.split_once('=').map_or(*right, |(name, _)| name);
            left.cmp(right)
        });
    }
    pairs.join("&")
}

/// Iterates `name[=value]` directives of a comma-separated header.
pub fn header_directives(
    headers: &HeaderMap,
    name: HeaderName,
) -> impl Iterator<Item = (&str, Option<&str>)> {
    headers
        .get_all(name)
        .into_iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|directive| !directive.is_empty())
        .map(|directive| match directive.split_once('=') {
            Some((name, value)) => (name.trim(), Some(value.trim().trim_matches('"'))),
            None => (directive, None),
        })
}

#[derive(Clone, Debug)]
pub struct ResponseRules {
    /// Status codes that may be stored in header-driven mode.
    pub statuses: Vec<u16>,
    /// TTL for the legacy explicit paths when the response sets none.
    pub default_ttl: Duration,
    pub default_stale_while_revalidate: Duration,
    /// Longest TTL or stale window honoured (caps hostile or typo'd values).
    pub max_ttl: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorePolicy {
    pub ttl: Duration,
    pub stale_while_revalidate: Duration,
}

/// Why a response is not stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotStored {
    Status,
    SetCookie,
    Private,
    NoStore,
    NoCache,
    VaryStar,
    NoFreshness,
    Partial,
}

/// Decides whether a response may be stored and for how long.
pub fn store_policy(
    rules: &ResponseRules,
    forced_path: bool,
    status: StatusCode,
    headers: &HeaderMap,
) -> Result<StorePolicy, NotStored> {
    if status == StatusCode::PARTIAL_CONTENT || headers.contains_key(http::header::CONTENT_RANGE) {
        return Err(NotStored::Partial);
    }
    let status_allowed = if forced_path && !rules.statuses.contains(&status.as_u16()) {
        status == StatusCode::OK
    } else {
        rules.statuses.contains(&status.as_u16())
    };
    if !status_allowed {
        return Err(NotStored::Status);
    }
    if headers.contains_key(http::header::SET_COOKIE) {
        return Err(NotStored::SetCookie);
    }
    // Safety-relevant headers that cannot be parsed (obs-text, invalid
    // tokens) cannot be proven safe, so they make the response uncacheable.
    for (name, refusal) in [
        (http::header::CACHE_CONTROL, NotStored::Private),
        (http::header::PRAGMA, NotStored::NoCache),
        (http::header::VARY, NotStored::VaryStar),
    ] {
        if headers
            .get_all(name)
            .iter()
            .any(|value| value.to_str().is_err())
        {
            return Err(refusal);
        }
    }
    match vary_names(headers, false) {
        None => return Err(NotStored::VaryStar),
        Some(names) if names.iter().any(|name| name.as_str() == "*") => {
            return Err(NotStored::VaryStar);
        }
        Some(_) => {}
    }
    let mut public = false;
    let mut shared_max_age = None;
    let mut max_age = None;
    let mut stale = None;
    for (name, value) in header_directives(headers, http::header::CACHE_CONTROL) {
        let name = name.to_ascii_lowercase();
        match name.as_str() {
            "private" => return Err(NotStored::Private),
            "no-store" => return Err(NotStored::NoStore),
            "no-cache" => return Err(NotStored::NoCache),
            "public" => public = true,
            "s-maxage" => shared_max_age = value.and_then(parse_seconds),
            "max-age" => max_age = value.and_then(parse_seconds),
            "stale-while-revalidate" => stale = value.and_then(parse_seconds),
            _ => {}
        }
    }
    if header_directives(headers, http::header::PRAGMA)
        .any(|(name, _)| name.eq_ignore_ascii_case("no-cache"))
    {
        return Err(NotStored::NoCache);
    }
    let ttl = shared_max_age
        .or(if public { max_age } else { None })
        .filter(|ttl| !ttl.is_zero());
    let (ttl, default_stale) = match (ttl, forced_path) {
        // Legacy explicit paths keep their configured TTL and stale window.
        (_, true) => (rules.default_ttl, rules.default_stale_while_revalidate),
        (Some(ttl), false) => (ttl, Duration::ZERO),
        (None, false) => return Err(NotStored::NoFreshness),
    };
    if ttl.is_zero() {
        return Err(NotStored::NoFreshness);
    }
    Ok(StorePolicy {
        ttl: ttl.min(rules.max_ttl),
        stale_while_revalidate: stale.unwrap_or(default_stale).min(rules.max_ttl),
    })
}

fn parse_seconds(value: &str) -> Option<Duration> {
    if value.is_empty() || value.len() > 10 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<u64>().ok().map(Duration::from_secs)
}

/// The request header names a stored variant must match, from `Vary`.
/// `Accept-Encoding` is dropped when the cache negotiates the encoding itself
/// (the stored body is identity), since every stored encoding is equivalent.
pub fn vary_names(headers: &HeaderMap, cache_negotiates_encoding: bool) -> Option<Vec<HeaderName>> {
    let mut names = Vec::new();
    for value in headers.get_all(http::header::VARY) {
        let value = value.to_str().ok()?;
        for name in value
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let name = HeaderName::try_from(name.to_ascii_lowercase()).ok()?;
            if cache_negotiates_encoding && name == http::header::ACCEPT_ENCODING {
                continue;
            }
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    (names.len() <= MAX_VARY_HEADERS).then_some(names)
}

/// The request's value for each vary name: every field line, joined.
pub fn vary_values(headers: &HeaderMap, names: &[HeaderName]) -> Vec<Option<Vec<u8>>> {
    names
        .iter()
        .map(|name| {
            let mut values = headers.get_all(name).iter().peekable();
            values.peek()?;
            let mut joined = Vec::new();
            for (index, value) in values.enumerate() {
                if index > 0 {
                    joined.extend_from_slice(b"\n");
                }
                joined.extend_from_slice(value.as_bytes());
            }
            Some(joined)
        })
        .collect()
}

/// Whether the request's value for `name` (every field line, joined by `\n`)
/// equals `stored`, without allocating.
pub fn vary_value_matches(headers: &HeaderMap, name: &HeaderName, stored: Option<&[u8]>) -> bool {
    let mut values = headers.get_all(name).iter().peekable();
    let Some(mut rest) = stored else {
        return values.peek().is_none();
    };
    let mut first = true;
    for value in values {
        if !first {
            match rest.strip_prefix(b"\n") {
                Some(remaining) => rest = remaining,
                None => return false,
            }
        }
        first = false;
        match rest.strip_prefix(value.as_bytes()) {
            Some(remaining) => rest = remaining,
            None => return false,
        }
    }
    !first && rest.is_empty()
}

/// Cache tags from the response's tag headers (comma or space separated).
pub fn parse_tags<'a>(values: impl Iterator<Item = &'a HeaderValue>) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for value in values {
        let Ok(value) = value.to_str() else { continue };
        for tag in value.split([',', ' ', '\t']).filter(|tag| valid_tag(tag)) {
            if tags.len() >= MAX_TAGS {
                return tags;
            }
            if !tags.iter().any(|existing| existing == tag) {
                tags.push(tag.to_owned());
            }
        }
    }
    tags
}

pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= MAX_TAG_BYTES
        && tag.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
}

/// A strong validator for a stored body.
pub fn etag_for(body: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(body);
    let mut tag = String::with_capacity(38);
    tag.push_str("\"pam-");
    for byte in &digest[..12] {
        tag.push_str(&format!("{byte:02x}"));
    }
    tag.push('"');
    tag
}

/// The ETag of an encoded representation of a stored body.
pub fn encoded_etag(etag: &str, encoding: &str) -> String {
    match etag.strip_suffix('"') {
        Some(prefix) => format!("{prefix}-{encoding}\""),
        None => format!("{etag}-{encoding}"),
    }
}

/// `If-None-Match` weak comparison against any of the given validators.
pub fn if_none_match_matches(headers: &HeaderMap, etags: &[&str]) -> bool {
    let strip = |tag: &str| tag.trim().trim_start_matches("W/").to_owned();
    headers
        .get_all(http::header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .any(|tag| tag == "*" || etags.iter().any(|etag| strip(etag) == strip(tag)))
}

/// `If-Modified-Since` exact match on the stored `Last-Modified` (browsers
/// echo the value they received); only used without `If-None-Match`.
pub fn not_modified_since(request: &HeaderMap, stored: &HeaderMap) -> bool {
    if request.contains_key(http::header::IF_NONE_MATCH) {
        return false;
    }
    match (
        request.get(http::header::IF_MODIFIED_SINCE),
        stored.get(http::header::LAST_MODIFIED),
    ) {
        (Some(since), Some(modified)) => since.as_bytes() == modified.as_bytes(),
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Encoding {
    Identity,
    Gzip,
    Brotli,
}

impl Encoding {
    pub fn token(self) -> &'static str {
        match self {
            Self::Identity => "identity",
            Self::Gzip => "gzip",
            Self::Brotli => "br",
        }
    }
}

/// Picks the best available encoding the client accepts (`q` > 0).
pub fn negotiate_encoding(headers: &HeaderMap, gzip: bool, brotli: bool) -> Encoding {
    let mut accepts_gzip = None;
    let mut accepts_brotli = None;
    let mut wildcard = None;
    for (coding, quality) in headers
        .get_all(http::header::ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|item| {
            let mut parts = item.split(';');
            let coding = parts.next().unwrap_or("").trim().to_ascii_lowercase();
            let quality = parts
                .filter_map(|parameter| {
                    let (name, value) = parameter.split_once('=')?;
                    name.trim()
                        .eq_ignore_ascii_case("q")
                        .then(|| value.trim().parse::<f32>().ok())
                        .flatten()
                })
                .next()
                .unwrap_or(1.0);
            (coding, quality > 0.0)
        })
    {
        match coding.as_str() {
            "br" => accepts_brotli = Some(quality),
            "gzip" | "x-gzip" => accepts_gzip = Some(quality),
            "*" => wildcard = Some(quality),
            _ => {}
        }
    }
    if brotli && accepts_brotli.or(wildcard).unwrap_or(false) {
        Encoding::Brotli
    } else if gzip && accepts_gzip.or(wildcard).unwrap_or(false) {
        Encoding::Gzip
    } else {
        Encoding::Identity
    }
}

/// Whether a stored body is worth compressing.
pub fn compressible(headers: &HeaderMap, body_len: usize) -> bool {
    if body_len < 1024 || headers.contains_key(http::header::CONTENT_ENCODING) {
        return false;
    }
    let Some(content_type) = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let content_type = content_type.to_ascii_lowercase();
    content_type.starts_with("text/")
        || [
            "json",
            "javascript",
            "xml",
            "svg",
            "wasm",
            "ecmascript",
            "csv",
        ]
        .iter()
        .any(|kind| content_type.contains(kind))
}

/// Response headers never stored: hop-by-hop fields and values the server
/// recomputes per response.
pub fn stored_header(name: &HeaderName) -> bool {
    !matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-connection"
            | "transfer-encoding"
            | "te"
            | "trailer"
            | "upgrade"
            | "content-length"
            | "date"
            | "age"
            | "set-cookie"
            | "x-pam-cache"
    )
}

/// A purge request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Purge {
    All,
    Tag(String),
    Url(UrlPattern),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UrlPattern {
    pub host: Option<String>,
    pub path: String,
    /// `None` matches any query; `Some` matches the normalized query exactly.
    pub query: Option<String>,
    /// A trailing `*` on the path purges every path with that prefix.
    pub prefix: bool,
}

impl UrlPattern {
    pub fn parse(url: &str, ignored: &[String]) -> Option<Self> {
        let url = url.trim();
        // Request targets are visible ASCII (hyper rejects anything else), so
        // whitespace or raw UTF-8 could never match and would not round-trip
        // through the invalidation log (found by fuzzing).
        if url.is_empty() || url.len() > 8192 || !url.bytes().all(|byte| byte.is_ascii_graphic()) {
            return None;
        }
        let (host, rest) = match url.split_once("://") {
            Some(("http" | "https", rest)) => {
                let (authority, rest) = rest
                    .find('/')
                    .map_or((rest, "/"), |index| rest.split_at(index));
                if authority.is_empty() {
                    return None;
                }
                (Some(authority.to_ascii_lowercase()), rest)
            }
            Some(_) => return None,
            None => (None, url),
        };
        if !rest.starts_with('/') {
            return None;
        }
        let (path, query) = match rest.split_once('?') {
            Some((path, query)) => (path, Some(normalize_query(query, ignored))),
            None => (rest, None),
        };
        let (path, prefix) = match path.strip_suffix('*') {
            Some(path) if query.is_none() => (path, true),
            Some(_) => return None,
            None => (path, false),
        };
        Some(Self {
            host,
            path: path.to_owned(),
            query,
            prefix,
        })
    }

    pub fn matches(&self, host: &str, path: &str, query: &str) -> bool {
        self.host.as_deref().is_none_or(|expected| expected == host)
            && if self.prefix {
                path.starts_with(&self.path)
            } else {
                path == self.path
            }
            && self
                .query
                .as_deref()
                .is_none_or(|expected| expected == query)
    }
}

impl Purge {
    /// Parses one invalidation-log line: `*`, a tag, or `!url <url>`.
    pub fn parse_line(line: &str, ignored: &[String]) -> Option<Self> {
        let line = line.trim();
        if line == "*" {
            Some(Self::All)
        } else if let Some(url) = line.strip_prefix("!url ") {
            UrlPattern::parse(url, ignored).map(Self::Url)
        } else if valid_tag(line) {
            Some(Self::Tag(line.to_owned()))
        } else {
            None
        }
    }

    pub fn to_line(&self) -> String {
        match self {
            Self::All => "*".to_owned(),
            Self::Tag(tag) => tag.clone(),
            Self::Url(pattern) => {
                let mut line = String::from("!url ");
                if let Some(host) = &pattern.host {
                    line.push_str("http://");
                    line.push_str(host);
                }
                line.push_str(&pattern.path);
                if pattern.prefix {
                    line.push('*');
                }
                if let Some(query) = &pattern.query {
                    line.push('?');
                    line.push_str(query);
                }
                line
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> RequestRules {
        RequestRules {
            header_driven: true,
            paths: Vec::new(),
            allow_cookies: false,
            allow_authorization: false,
            ignored_query_params: vec!["utm_*".to_owned(), "fbclid".to_owned()],
            vary_headers: Vec::new(),
        }
    }

    fn response_rules() -> ResponseRules {
        ResponseRules {
            statuses: vec![200, 203, 204, 300, 301, 308, 404, 410],
            default_ttl: Duration::from_secs(30),
            default_stale_while_revalidate: Duration::ZERO,
            max_ttl: Duration::from_secs(86_400 * 365),
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

    fn key(
        rules: &RequestRules,
        method: Method,
        query: Option<&str>,
        request: &HeaderMap,
    ) -> Result<RequestKey, Bypass> {
        request_key(
            rules,
            &method,
            "http",
            Some("Example.com"),
            "/page",
            query,
            request,
        )
    }

    #[test]
    fn credentials_bypass_unless_explicitly_allowed() {
        let mut rules = rules();
        assert_eq!(
            key(&rules, Method::GET, None, &headers(&[("cookie", "a=1")])),
            Err(Bypass::Cookie)
        );
        assert_eq!(
            key(
                &rules,
                Method::GET,
                None,
                &headers(&[("authorization", "Bearer x")])
            ),
            Err(Bypass::Authorization)
        );
        rules.allow_cookies = true;
        assert!(key(&rules, Method::GET, None, &headers(&[("cookie", "a=1")])).is_ok());
        assert_eq!(
            key(
                &rules,
                Method::GET,
                None,
                &headers(&[("authorization", "Bearer x")])
            ),
            Err(Bypass::Authorization)
        );
        rules.allow_authorization = true;
        assert!(
            key(
                &rules,
                Method::GET,
                None,
                &headers(&[("authorization", "Bearer x")])
            )
            .is_ok()
        );
    }

    #[test]
    fn only_get_and_head_without_ranges_or_upgrades() {
        let rules = rules();
        for method in [
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ] {
            assert_eq!(
                key(&rules, method, None, &HeaderMap::new()),
                Err(Bypass::Method)
            );
        }
        assert!(
            key(&rules, Method::HEAD, None, &HeaderMap::new())
                .unwrap()
                .head
        );
        assert_eq!(
            key(
                &rules,
                Method::GET,
                None,
                &headers(&[("range", "bytes=0-1")])
            ),
            Err(Bypass::Range)
        );
        assert_eq!(
            key(
                &rules,
                Method::GET,
                None,
                &headers(&[("upgrade", "websocket")])
            ),
            Err(Bypass::Upgrade)
        );
        let disabled = RequestRules {
            header_driven: false,
            ..rules
        };
        assert_eq!(
            key(&disabled, Method::GET, None, &HeaderMap::new()),
            Err(Bypass::Disabled)
        );
    }

    #[test]
    fn keys_separate_hosts_schemes_queries_and_forwarded_headers() {
        let rules = rules();
        let base = key(&rules, Method::GET, Some("a=1&b=2"), &HeaderMap::new()).unwrap();
        assert_eq!(base.host, "example.com");
        let reordered = key(
            &rules,
            Method::GET,
            Some("b=2&a=1&utm_source=x&fbclid=y"),
            &HeaderMap::new(),
        )
        .unwrap();
        assert_eq!(base.primary, reordered.primary);
        let other_query = key(&rules, Method::GET, Some("a=1&b=3"), &HeaderMap::new()).unwrap();
        assert_ne!(base.primary, other_query.primary);
        let forwarded = key(
            &rules,
            Method::GET,
            Some("a=1&b=2"),
            &headers(&[("x-forwarded-host", "evil.test")]),
        )
        .unwrap();
        assert_ne!(base.primary, forwarded.primary);
        let proto = key(
            &rules,
            Method::GET,
            Some("a=1&b=2"),
            &headers(&[("x-forwarded-proto", "https")]),
        )
        .unwrap();
        assert_ne!(base.primary, proto.primary);
        let https = request_key(
            &rules,
            &Method::GET,
            "https",
            Some("example.com"),
            "/page",
            Some("a=1&b=2"),
            &HeaderMap::new(),
        )
        .unwrap();
        assert_ne!(base.primary, https.primary);
        let other_host = request_key(
            &rules,
            &Method::GET,
            "http",
            Some("other.com"),
            "/page",
            Some("a=1&b=2"),
            &HeaderMap::new(),
        )
        .unwrap();
        assert_ne!(base.primary, other_host.primary);
        let head = key(&rules, Method::HEAD, Some("a=1&b=2"), &HeaderMap::new()).unwrap();
        assert_eq!(base.primary, head.primary);
    }

    #[test]
    fn query_order_is_kept_whenever_php_could_observe_it() {
        assert_eq!(normalize_query("b=1&a=2", &[]), "a=2&b=1");
        assert_eq!(normalize_query("a=1&a=2", &[]), "a=1&a=2");
        assert_eq!(normalize_query("a=2&a=1", &[]), "a=2&a=1");
        assert_eq!(normalize_query("x[]=2&x[]=1", &[]), "x[]=2&x[]=1");
        assert_eq!(normalize_query("b=1&a.b=2", &[]), "b=1&a.b=2");
        assert_eq!(normalize_query("b=1&%61=2", &[]), "b=1&%61=2");
        assert_eq!(normalize_query("&&b=1&", &[]), "b=1");
        assert_ne!(
            normalize_query("a_b=1&a.b=2", &[]),
            normalize_query("a.b=2&a_b=1", &[])
        );
    }

    #[test]
    fn keyed_vary_headers_are_length_prefixed() {
        let mut rules = rules();
        rules.vary_headers = vec![HeaderName::from_static("accept-language")];
        let a = key(
            &rules,
            Method::GET,
            None,
            &headers(&[("accept-language", "en"), ("accept-language", "pt")]),
        )
        .unwrap();
        let b = key(
            &rules,
            Method::GET,
            None,
            &headers(&[("accept-language", "en, pt")]),
        )
        .unwrap();
        let c = key(&rules, Method::GET, None, &HeaderMap::new()).unwrap();
        assert_ne!(a.primary, b.primary);
        assert_ne!(a.primary, c.primary);
    }

    #[test]
    fn stores_only_explicitly_public_responses() {
        let rules = response_rules();
        let ok =
            |pairs: &[(&str, &str)]| store_policy(&rules, false, StatusCode::OK, &headers(pairs));
        assert_eq!(
            ok(&[(
                "cache-control",
                "public, s-maxage=60, stale-while-revalidate=30"
            )]),
            Ok(StorePolicy {
                ttl: Duration::from_secs(60),
                stale_while_revalidate: Duration::from_secs(30)
            })
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=5")]).unwrap().ttl,
            Duration::from_secs(5)
        );
        assert_eq!(
            ok(&[("cache-control", "public, max-age=7")]).unwrap().ttl,
            Duration::from_secs(7)
        );
        assert_eq!(
            ok(&[("cache-control", "max-age=7")]),
            Err(NotStored::NoFreshness)
        );
        assert_eq!(ok(&[]), Err(NotStored::NoFreshness));
        assert_eq!(
            ok(&[("cache-control", "public, s-maxage=0")]),
            Err(NotStored::NoFreshness)
        );
        assert_eq!(
            ok(&[("cache-control", "public, s-maxage=60, private")]),
            Err(NotStored::Private)
        );
        assert_eq!(
            ok(&[
                ("cache-control", "public, s-maxage=60"),
                ("cache-control", "no-store")
            ]),
            Err(NotStored::NoStore)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=60, no-cache")]),
            Err(NotStored::NoCache)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=60, no-cache=\"set-cookie\"")]),
            Err(NotStored::NoCache)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=60"), ("pragma", "no-cache")]),
            Err(NotStored::NoCache)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=60"), ("set-cookie", "a=1")]),
            Err(NotStored::SetCookie)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=60"), ("vary", "Accept, *")]),
            Err(NotStored::VaryStar)
        );
        assert_eq!(
            ok(&[
                ("cache-control", "s-maxage=60"),
                ("content-range", "bytes 0-1/2")
            ]),
            Err(NotStored::Partial)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=abc")]),
            Err(NotStored::NoFreshness)
        );
        assert_eq!(
            ok(&[("cache-control", "s-maxage=-1")]),
            Err(NotStored::NoFreshness)
        );
        let mut invalid = headers(&[("cache-control", "s-maxage=60")]);
        invalid.append(
            "cache-control",
            HeaderValue::from_bytes(b"priv\xffate").unwrap(),
        );
        assert_eq!(
            store_policy(&rules, false, StatusCode::OK, &invalid),
            Err(NotStored::Private)
        );
        // Found by fuzzing: unparsable Pragma/Vary values were skipped.
        for (name, value, refusal) in [
            ("pragma", &b"x\xe3, no-cache"[..], NotStored::NoCache),
            ("vary", &b"\xe9ccept"[..], NotStored::VaryStar),
        ] {
            let mut invalid = headers(&[("cache-control", "s-maxage=60")]);
            invalid.append(
                HeaderName::from_static(name),
                HeaderValue::from_bytes(value).unwrap(),
            );
            assert_eq!(
                store_policy(&rules, false, StatusCode::OK, &invalid),
                Err(refusal),
                "{name}"
            );
        }
        for vary in ["*=60", "a b", "Accept, *"] {
            assert_eq!(
                ok(&[("cache-control", "s-maxage=60"), ("vary", vary)]),
                Err(NotStored::VaryStar),
                "{vary}"
            );
        }
    }

    #[test]
    fn status_policy() {
        let rules = response_rules();
        let public = headers(&[("cache-control", "public, s-maxage=60")]);
        for status in [200, 301, 404, 410] {
            assert!(
                store_policy(
                    &rules,
                    false,
                    StatusCode::from_u16(status).unwrap(),
                    &public
                )
                .is_ok()
            );
        }
        for status in [201, 206, 302, 304, 400, 401, 403, 500, 502, 503] {
            assert!(
                store_policy(
                    &rules,
                    false,
                    StatusCode::from_u16(status).unwrap(),
                    &public
                )
                .is_err(),
                "{status}"
            );
        }
    }

    #[test]
    fn legacy_paths_use_the_configured_ttl() {
        let mut rules = response_rules();
        rules.default_ttl = Duration::from_millis(100);
        rules.default_stale_while_revalidate = Duration::from_secs(2);
        let policy = store_policy(
            &rules,
            true,
            StatusCode::OK,
            &headers(&[("cache-control", "public, max-age=30")]),
        )
        .unwrap();
        assert_eq!(policy.ttl, Duration::from_millis(100));
        assert_eq!(policy.stale_while_revalidate, Duration::from_secs(2));
        assert!(store_policy(&rules, true, StatusCode::OK, &HeaderMap::new()).is_ok());
        assert_eq!(
            store_policy(
                &rules,
                true,
                StatusCode::OK,
                &headers(&[("cache-control", "private")])
            ),
            Err(NotStored::Private)
        );
        assert_eq!(
            store_policy(
                &rules,
                true,
                StatusCode::OK,
                &headers(&[("set-cookie", "a=b")])
            ),
            Err(NotStored::SetCookie)
        );
    }

    #[test]
    fn vary_parsing() {
        let response = headers(&[
            ("vary", "Accept-Language, accept-encoding"),
            ("vary", "X-Tenant"),
        ]);
        let names = vary_names(&response, true).unwrap();
        assert_eq!(
            names,
            vec![
                HeaderName::from_static("accept-language"),
                HeaderName::from_static("x-tenant")
            ]
        );
        assert_eq!(vary_names(&response, false).unwrap().len(), 3);
        let request = headers(&[("x-tenant", "a"), ("x-tenant", "b")]);
        let values = vary_values(&request, &names);
        assert_eq!(values, vec![None, Some(b"a\nb".to_vec())]);
        for (name, value) in names.iter().zip(&values) {
            assert!(vary_value_matches(&request, name, value.as_deref()));
        }
        let tenant = HeaderName::from_static("x-tenant");
        assert!(!vary_value_matches(&request, &tenant, Some(b"a")));
        assert!(!vary_value_matches(&request, &tenant, Some(b"a\nb\n")));
        assert!(!vary_value_matches(&request, &tenant, Some(b"ab")));
        assert!(!vary_value_matches(&request, &tenant, None));
        assert!(!vary_value_matches(
            &headers(&[("x-tenant", "")]),
            &tenant,
            None
        ));
        assert!(vary_value_matches(
            &headers(&[("x-tenant", "")]),
            &tenant,
            Some(b"")
        ));
        assert!(!vary_value_matches(&HeaderMap::new(), &tenant, Some(b"")));
    }

    #[test]
    fn conditional_requests() {
        let etag = "\"pam-abc\"";
        let br = encoded_etag(etag, "br");
        assert_eq!(br, "\"pam-abc-br\"");
        assert!(if_none_match_matches(
            &headers(&[("if-none-match", "\"x\", W/\"pam-abc\"")]),
            &[etag]
        ));
        assert!(if_none_match_matches(
            &headers(&[("if-none-match", "*")]),
            &[etag]
        ));
        assert!(if_none_match_matches(
            &headers(&[("if-none-match", "\"pam-abc-br\"")]),
            &[etag, &br]
        ));
        assert!(!if_none_match_matches(
            &headers(&[("if-none-match", "\"pam-abd\"")]),
            &[etag, &br]
        ));
        assert!(!if_none_match_matches(&HeaderMap::new(), &[etag]));
        let stored = headers(&[("last-modified", "Tue, 10 Nov 2026 10:00:00 GMT")]);
        assert!(not_modified_since(
            &headers(&[("if-modified-since", "Tue, 10 Nov 2026 10:00:00 GMT")]),
            &stored
        ));
        assert!(!not_modified_since(
            &headers(&[("if-modified-since", "Tue, 10 Nov 2026 10:00:01 GMT")]),
            &stored
        ));
    }

    #[test]
    fn encoding_negotiation() {
        let accept = |value: &str| headers(&[("accept-encoding", value)]);
        assert_eq!(
            negotiate_encoding(&accept("gzip, deflate, br"), true, true),
            Encoding::Brotli
        );
        assert_eq!(
            negotiate_encoding(&accept("gzip, br;q=0"), true, true),
            Encoding::Gzip
        );
        assert_eq!(
            negotiate_encoding(&accept("gzip;q=0, br;q=0"), true, true),
            Encoding::Identity
        );
        assert_eq!(
            negotiate_encoding(&accept("*"), true, false),
            Encoding::Gzip
        );
        assert_eq!(
            negotiate_encoding(&HeaderMap::new(), true, true),
            Encoding::Identity
        );
        assert_eq!(
            negotiate_encoding(&accept("br"), true, false),
            Encoding::Identity
        );
    }

    #[test]
    fn purge_lines_round_trip() {
        let ignored = vec!["utm_*".to_owned()];
        for line in [
            "*",
            "products",
            "product:42",
            "!url /products/42",
            "!url /products/*",
            "!url /a?b=1&a=2",
            "!url http://example.com/x",
        ] {
            let purge = Purge::parse_line(line, &ignored).unwrap();
            assert_eq!(
                Purge::parse_line(&purge.to_line(), &ignored),
                Some(purge),
                "{line}"
            );
        }
        assert_eq!(Purge::parse_line("bad tag", &ignored), None);
        assert_eq!(Purge::parse_line("!url relative", &ignored), None);
        assert_eq!(Purge::parse_line("!url ftp://x/y", &ignored), None);
        assert_eq!(Purge::parse_line("!url /a?b=1 &&", &ignored), None);
        assert_eq!(Purge::parse_line("!url /caf\u{e9}", &ignored), None);
        let pattern = UrlPattern::parse("https://Example.com/products/*", &ignored).unwrap();
        assert!(pattern.matches("example.com", "/products/42", "page=2"));
        assert!(!pattern.matches("other.com", "/products/42", ""));
        let exact = UrlPattern::parse("/p?b=1&a=2&utm_x=1", &ignored).unwrap();
        assert!(exact.matches("any", "/p", "a=2&b=1"));
        assert!(!exact.matches("any", "/p", "a=2"));
        let any_query = UrlPattern::parse("/p", &ignored).unwrap();
        assert!(any_query.matches("any", "/p", "a=2"));
        assert!(!any_query.matches("any", "/p/x", ""));
    }

    #[test]
    fn tags_are_validated_and_bounded() {
        let values = [HeaderValue::from_static(
            "products, product:42 bad!tag products",
        )];
        assert_eq!(
            parse_tags(values.iter()),
            vec!["products".to_owned(), "product:42".to_owned()]
        );
        let many = HeaderValue::from_str(
            &(0..200)
                .map(|index| format!("t{index}"))
                .collect::<Vec<_>>()
                .join(","),
        )
        .unwrap();
        assert_eq!(parse_tags(std::iter::once(&many)).len(), MAX_TAGS);
    }
}
