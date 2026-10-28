//! Downloads a file for URL import without becoming an SSRF hole.
//!
//! Only `http` and `https` URLs without credentials are fetched. Every address
//! the client connects to, including after a redirect, must be public unless
//! private addresses are allowed: literal IPs are checked before the request
//! and on each redirect, and host names resolve through a resolver that drops
//! non-public addresses. Proxies from the environment are ignored, the body is
//! capped while it streams, and slow servers time out.

use std::error::Error as StdError;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use reqwest::{Url, redirect};

const MAX_REDIRECTS: usize = 5;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Longest wait for the next piece of the response.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("StarBooru/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("enter an http or https address")]
    BadUrl,
    #[error("addresses on a private network can't be imported")]
    Blocked,
    #[error("the file is larger than the server limit")]
    TooLarge,
    #[error("that address answered with HTTP {0}")]
    Status(u16),
    #[error("the request timed out")]
    Timeout,
    #[error("too many redirects")]
    Redirects,
    #[error("can't reach that address")]
    Unreachable,
}

/// The rule for which addresses may be fetched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Public,
    /// Also private, loopback and link-local addresses (a trusted LAN).
    Any,
}

pub struct Fetcher {
    client: reqwest::Client,
    reach: Reach,
}

impl Fetcher {
    pub fn new(reach: Reach) -> Result<Self, reqwest::Error> {
        // sqlx already uses ring; reqwest takes the process-wide provider.
        // Err means one is installed already, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .no_proxy()
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .dns_resolver(Arc::new(Resolver { reach }))
            .redirect(redirect::Policy::custom(move |attempt| {
                if attempt.previous().len() >= MAX_REDIRECTS {
                    attempt.error(TooManyRedirects)
                } else if check_url(attempt.url(), reach).is_err() {
                    attempt.error(Blocked)
                } else {
                    attempt.follow()
                }
            }))
            .build()?;
        Ok(Self { client, reach })
    }

    /// Whether `raw` could be fetched, without fetching it (host names are
    /// still checked when they resolve).
    pub fn check(&self, raw: &str) -> Result<(), FetchError> {
        let url = Url::parse(raw.trim()).map_err(|_| FetchError::BadUrl)?;
        check_url(&url, self.reach)
    }

    /// Downloads `raw` into memory, refusing bodies over `max_bytes`.
    pub async fn get(&self, raw: &str, max_bytes: usize) -> Result<Vec<u8>, FetchError> {
        let url = Url::parse(raw.trim()).map_err(|_| FetchError::BadUrl)?;
        check_url(&url, self.reach)?;
        let mut response = self.client.get(url).send().await.map_err(classify)?;
        let status = response.status();
        if !status.is_success() {
            return Err(FetchError::Status(status.as_u16()));
        }
        let max = u64::try_from(max_bytes).unwrap_or(u64::MAX);
        if response.content_length().is_some_and(|len| len > max) {
            return Err(FetchError::TooLarge);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(classify)? {
            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                return Err(FetchError::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }
}

/// Scheme, credentials and (for literal IPs) the address. Host names are
/// checked when they resolve.
fn check_url(url: &Url, reach: Reach) -> Result<(), FetchError> {
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(FetchError::BadUrl);
    }
    let ip = match url.host() {
        None => return Err(FetchError::BadUrl),
        Some(url::Host::Domain(_)) => return Ok(()),
        Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip),
        Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip),
    };
    if allowed(ip, reach) {
        Ok(())
    } else {
        Err(FetchError::Blocked)
    }
}

fn allowed(ip: IpAddr, reach: Reach) -> bool {
    reach == Reach::Any || is_public(ip)
}

/// Globally routable unicast. Everything else (private, loopback,
/// link-local, shared, documentation, multicast, reserved, and IPv6 forms
/// that wrap such an IPv4 address) is not.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0                                   // "this network"
        || a == 10                             // private
        || a == 127                            // loopback
        || (a == 100 && (64..128).contains(&b)) // shared (CGNAT)
        || (a == 169 && b == 254)              // link-local
        || (a == 172 && (16..32).contains(&b)) // private
        || (a == 192 && b == 0 && c == 0)      // IETF protocol assignments
        || (a == 192 && b == 0 && c == 2)      // documentation
        || (a == 192 && b == 88 && c == 99)    // 6to4 relay
        || (a == 192 && b == 168)              // private
        || (a == 198 && (18..20).contains(&b)) // benchmarking
        || (a == 198 && b == 51 && c == 100)   // documentation
        || (a == 203 && b == 0 && c == 113)    // documentation
        || a >= 224) // multicast, reserved, broadcast
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    // IPv4-mapped (::ffff:a.b.c.d) reaches IPv4.
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    match ip.segments() {
        // NAT64 (64:ff9b::a.b.c.d) and 6to4 (2002:aabb:ccdd::) wrap IPv4.
        [0x64, 0xff9b, 0, 0, 0, 0, hi, lo] => is_public_v4(embedded_v4(hi, lo)),
        [0x2002, hi, lo, ..] => is_public_v4(embedded_v4(hi, lo)),
        // Within global unicast (2000::/3): Teredo, benchmarking, ORCHID
        // and documentation are not routable.
        [0x2001, 0, ..] | [0x2001, 0x2, 0, ..] | [0x2001, 0xdb8, ..] => false,
        [0x2001, b, ..] if (0x10..0x30).contains(&b) => false,
        [0x3fff, b, ..] if b < 0x1000 => false,
        [a, ..] => (a & 0xe000) == 0x2000,
    }
}

fn embedded_v4(hi: u16, lo: u16) -> Ipv4Addr {
    let [a, b] = hi.to_be_bytes();
    let [c, d] = lo.to_be_bytes();
    Ipv4Addr::new(a, b, c, d)
}

/// System DNS, minus the addresses `reach` forbids.
struct Resolver {
    reach: Reach,
}

impl Resolve for Resolver {
    fn resolve(&self, name: Name) -> Resolving {
        let reach = self.reach;
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let found: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), 0))
                .await?
                .filter(|a| allowed(a.ip(), reach))
                .collect();
            if found.is_empty() {
                return Err(Box::new(Blocked) as Box<dyn StdError + Send + Sync>);
            }
            Ok(Box::new(found.into_iter()) as Addrs)
        })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("address not allowed")]
struct Blocked;

#[derive(Debug, thiserror::Error)]
#[error("too many redirects")]
struct TooManyRedirects;

/// Maps a reqwest error to something the user can act on.
fn classify(e: reqwest::Error) -> FetchError {
    let mut source: Option<&(dyn StdError + 'static)> = Some(&e);
    while let Some(err) = source {
        if err.is::<Blocked>() {
            return FetchError::Blocked;
        }
        if err.is::<TooManyRedirects>() {
            return FetchError::Redirects;
        }
        source = err.source();
    }
    if e.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::Unreachable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(s: &str) -> bool {
        is_public(s.parse().unwrap())
    }

    #[test]
    fn only_public_addresses_are_public() {
        for ip in [
            "1.1.1.1",
            "8.8.8.8",
            "93.184.215.14",
            "2606:4700::1111",
            "2a00:1450::1",
        ] {
            assert!(public(ip), "{ip} is public");
        }
        for ip in [
            "0.0.0.0",
            "10.1.2.3",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.169.254",
            "172.16.0.1",
            "172.31.255.255",
            "192.0.0.8",
            "192.0.2.1",
            "192.168.1.1",
            "198.18.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "255.255.255.255",
            "::",
            "::1",
            "fc00::1",
            "fd12:3456::1",
            "fe80::1",
            "ff02::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::",
            "2001::1",
            "2001:db8::1",
            "::127.0.0.1",
        ] {
            assert!(!public(ip), "{ip} is not public");
        }
        // Wrapped public IPv4 stays public.
        assert!(public("::ffff:8.8.8.8"));
        assert!(public("2002:0808:0808::1"));
    }

    #[test]
    fn checks_urls() {
        let check = |u: &str, reach| check_url(&Url::parse(u).unwrap(), reach);
        assert!(check("https://example.com/a.png", Reach::Public).is_ok());
        assert!(matches!(
            check("ftp://example.com/a", Reach::Public),
            Err(FetchError::BadUrl)
        ));
        assert!(matches!(
            check("file:///etc/passwd", Reach::Any),
            Err(FetchError::BadUrl)
        ));
        assert!(matches!(
            check("https://user:pw@example.com/a", Reach::Public),
            Err(FetchError::BadUrl)
        ));
        assert!(matches!(
            check("http://127.0.0.1:5432/", Reach::Public),
            Err(FetchError::Blocked)
        ));
        assert!(matches!(
            check("http://[::1]/", Reach::Public),
            Err(FetchError::Blocked)
        ));
        // Decimal and hex IPv4 forms are normalised by the URL parser.
        assert!(matches!(
            check("http://2130706433/", Reach::Public),
            Err(FetchError::Blocked)
        ));
        assert!(matches!(
            check("http://0x7f.1/", Reach::Public),
            Err(FetchError::Blocked)
        ));
        assert!(check("http://127.0.0.1/", Reach::Any).is_ok());
    }

    #[tokio::test]
    async fn refuses_local_names_and_addresses() {
        let f = Fetcher::new(Reach::Public).unwrap();
        // `localhost` resolves, but only to loopback.
        assert!(matches!(
            f.get("http://localhost:1/x", 1024).await,
            Err(FetchError::Blocked)
        ));
        assert!(matches!(
            f.get("http://127.0.0.1:1/x", 1024).await,
            Err(FetchError::Blocked)
        ));
        assert!(matches!(
            f.get("not a url", 1024).await,
            Err(FetchError::BadUrl)
        ));
    }
}
