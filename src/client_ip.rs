//! Client address behind trusted proxies.
//!
//! A proxy appends the address it received the connection from to
//! `X-Forwarded-For`, so only the right end of the header is trustworthy: the
//! client is the first address, walking from the right, that was not added by
//! a trusted proxy. Everything to its left is whatever the client sent. Each
//! entry must parse as an IP address; anything else stops the walk.

use std::net::IpAddr;

use http::HeaderMap;

/// Environment variable with extra trusted proxies (comma separated
/// addresses or CIDR ranges), added to the `trustedProxies` option.
pub const TRUSTED_PROXIES_ENV: &str = "PAM_TRUSTED_PROXIES";

/// Most `X-Forwarded-For` entries examined per request.
const MAX_FORWARDED_HOPS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    Any,
    Network { address: IpAddr, prefix: u8 },
}

#[derive(Clone, Debug, Default)]
pub struct TrustedProxies {
    rules: Vec<Rule>,
}

impl TrustedProxies {
    /// Parses `*`, addresses and CIDR ranges (`10.0.0.0/8`, `fd00::/8`).
    pub fn parse<'a>(entries: impl IntoIterator<Item = &'a str>) -> Result<Self, String> {
        let mut rules = Vec::new();
        for entry in entries {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            rules.push(parse_rule(entry)?);
        }
        Ok(Self { rules })
    }

    /// The configured list plus [`TRUSTED_PROXIES_ENV`].
    pub fn from_config(configured: &[String]) -> Result<Self, String> {
        let environment = std::env::var(TRUSTED_PROXIES_ENV).unwrap_or_default();
        Self::parse(
            configured
                .iter()
                .map(String::as_str)
                .chain(environment.split(',')),
        )
    }

    /// Whether `*` is configured (every peer may set the client address).
    pub fn trusts_everyone(&self) -> bool {
        self.rules.contains(&Rule::Any)
    }

    pub fn trusts(&self, address: IpAddr) -> bool {
        let address = canonical(address);
        self.rules.iter().any(|rule| match *rule {
            Rule::Any => true,
            Rule::Network {
                address: network,
                prefix,
            } => in_network(address, network, prefix),
        })
    }

    /// The client address of a request received from `peer`.
    pub fn client_address(&self, peer: IpAddr, headers: &HeaderMap) -> IpAddr {
        let mut client = canonical(peer);
        if self.rules.is_empty() || !self.trusts(client) {
            return client;
        }
        let mut examined = 0;
        // Later header lines were appended by later proxies: walk them, and
        // the entries inside each, from the right.
        for value in headers.get_all("x-forwarded-for").iter().rev() {
            let Ok(value) = value.to_str() else {
                return client;
            };
            for entry in value.rsplit(',') {
                examined += 1;
                if examined > MAX_FORWARDED_HOPS {
                    return client;
                }
                let Some(address) = parse_forwarded_entry(entry) else {
                    return client;
                };
                client = address;
                if !self.trusts(client) {
                    return client;
                }
            }
        }
        client
    }
}

fn parse_rule(entry: &str) -> Result<Rule, String> {
    if entry == "*" {
        return Ok(Rule::Any);
    }
    let invalid = || format!("trusted proxy {entry:?} is not an IP address or CIDR range");
    let (address, prefix) = match entry.split_once('/') {
        Some((address, prefix)) => (address, Some(prefix)),
        None => (entry, None),
    };
    let address = canonical(address.parse::<IpAddr>().map_err(|_| invalid())?);
    let width = if address.is_ipv4() { 32 } else { 128 };
    let prefix = match prefix {
        Some(prefix) => prefix
            .parse::<u8>()
            .ok()
            .filter(|prefix| *prefix <= width)
            .ok_or_else(invalid)?,
        None => width,
    };
    Ok(Rule::Network { address, prefix })
}

/// An `X-Forwarded-For` entry: an address, optionally `[v6]:port` or
/// `v4:port` as some proxies write it.
fn parse_forwarded_entry(entry: &str) -> Option<IpAddr> {
    let entry = entry.trim();
    if entry.is_empty() || entry.len() > 64 {
        return None;
    }
    if let Ok(address) = entry.parse::<IpAddr>() {
        return Some(canonical(address));
    }
    if let Some(rest) = entry.strip_prefix('[') {
        let (address, _) = rest.split_once(']')?;
        return address.parse::<IpAddr>().ok().map(canonical);
    }
    let (address, port) = entry.rsplit_once(':')?;
    port.parse::<u16>().ok()?;
    address.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4)
}

/// IPv4-mapped IPv6 addresses (dual-stack listeners) compare as IPv4.
fn canonical(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    }
}

fn in_network(address: IpAddr, network: IpAddr, prefix: u8) -> bool {
    match (address, network) {
        (IpAddr::V4(address), IpAddr::V4(network)) => {
            let mask = u32::MAX.checked_shl(32 - u32::from(prefix)).unwrap_or(0);
            u32::from(address) & mask == u32::from(network) & mask
        }
        (IpAddr::V6(address), IpAddr::V6(network)) => {
            let mask = u128::MAX.checked_shl(128 - u32::from(prefix)).unwrap_or(0);
            u128::from(address) & mask == u128::from(network) & mask
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", value.parse().unwrap());
        }
        headers
    }

    fn ip(value: &str) -> IpAddr {
        value.parse().unwrap()
    }

    #[test]
    fn untrusted_peers_cannot_set_the_client_address() {
        let proxies = TrustedProxies::parse(["10.0.0.1"]).unwrap();
        assert_eq!(
            proxies.client_address(ip("203.0.113.7"), &headers(&["1.2.3.4"])),
            ip("203.0.113.7")
        );
        let none = TrustedProxies::default();
        assert_eq!(
            none.client_address(ip("10.0.0.1"), &headers(&["1.2.3.4"])),
            ip("10.0.0.1")
        );
    }

    #[test]
    fn the_client_is_the_rightmost_untrusted_hop() {
        let proxies = TrustedProxies::parse(["10.0.0.0/8"]).unwrap();
        // The client forged the left entry; the proxy appended the real one.
        assert_eq!(
            proxies.client_address(ip("10.0.0.1"), &headers(&["6.6.6.6, 203.0.113.9"])),
            ip("203.0.113.9")
        );
        // Two trusted hops, then the client.
        assert_eq!(
            proxies.client_address(
                ip("10.0.0.1"),
                &headers(&["6.6.6.6, 198.51.100.4, 10.1.2.3"])
            ),
            ip("198.51.100.4")
        );
        // Separate header lines count as one list, the last line last.
        assert_eq!(
            proxies.client_address(ip("10.0.0.1"), &headers(&["6.6.6.6", "198.51.100.4"])),
            ip("198.51.100.4")
        );
        // Only trusted proxies: the leftmost address.
        assert_eq!(
            proxies.client_address(ip("10.0.0.1"), &headers(&["10.9.9.9, 10.1.1.1"])),
            ip("10.9.9.9")
        );
    }

    #[test]
    fn invalid_entries_stop_at_the_last_trusted_hop() {
        let proxies = TrustedProxies::parse(["127.0.0.1"]).unwrap();
        assert_eq!(
            proxies.client_address(ip("127.0.0.1"), &headers(&["<script>"])),
            ip("127.0.0.1")
        );
        assert_eq!(
            proxies.client_address(ip("127.0.0.1"), &headers(&["1.2.3.4, unknown"])),
            ip("127.0.0.1")
        );
        assert_eq!(
            proxies.client_address(ip("127.0.0.1"), &headers(&["192.0.2.1:4711"])),
            ip("192.0.2.1")
        );
        assert_eq!(
            proxies.client_address(ip("127.0.0.1"), &headers(&["[2001:db8::1]:443"])),
            ip("2001:db8::1")
        );
    }

    #[test]
    fn parses_cidr_ranges_and_mapped_addresses() {
        let proxies = TrustedProxies::parse(["192.168.0.0/16", "fd00::/8", " "]).unwrap();
        assert!(proxies.trusts(ip("192.168.4.5")));
        assert!(!proxies.trusts(ip("192.169.0.1")));
        assert!(proxies.trusts(ip("fd12::1")));
        assert!(proxies.trusts(ip("::ffff:192.168.1.1")));
        assert!(!proxies.trusts_everyone());
        assert!(TrustedProxies::parse(["*"]).unwrap().trusts_everyone());
        assert!(TrustedProxies::parse(["10.0.0.0/33"]).is_err());
        assert!(TrustedProxies::parse(["proxy.internal"]).is_err());
        let all = TrustedProxies::parse(["0.0.0.0/0"]).unwrap();
        assert!(all.trusts(ip("8.8.8.8")));
    }

    #[test]
    fn bounds_the_number_of_examined_hops() {
        let proxies = TrustedProxies::parse(["*"]).unwrap();
        let chain = vec!["10.0.0.1"; 100].join(", ");
        let header = format!("1.2.3.4, {chain}");
        assert_eq!(
            proxies.client_address(ip("10.0.0.2"), &headers(&[&header])),
            ip("10.0.0.1")
        );
    }
}
