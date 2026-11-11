// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Per-request client-IP resolution and propagation.
//!
//! Two pieces:
//!
//! 1. `resolve_client_ip` — given the immediate peer address and the request
//!    headers, decide what to record as the originating client IP.
//!    `X-Forwarded-For` is only honoured when the peer is on a known proxy
//!    network (loopback or RFC1918). On a Docker Compose deployment, nginx is
//!    the only thing that can reach the backend port; in production cloud
//!    deploys this gate prevents header spoofing if the backend ever ends
//!    up directly reachable.
//!
//! 2. `CLIENT_IP` — a tokio task-local set by the middleware below before
//!    the handler runs. The audit writer reads it without needing every
//!    handler to thread it through. Tokio task-locals are carried across
//!    `.await` points within the same task, which covers all axum handler
//!    code paths (audit calls don't `tokio::spawn` away from the request).
//!
//! Together: nginx forwards the real client IP via X-Forwarded-For; the
//! middleware unwraps it; audit rows record the original client.
//!
//! Trust strategy: option (a) from the security plan — trust XFF only when
//! the immediate connection comes from a known proxy network. The list is
//! deliberately conservative (loopback + RFC1918) rather than reading from
//! env, since we only have one deploy topology today.

use std::net::{IpAddr, SocketAddr};

use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Request};
use axum::middleware::Next;
use axum::response::Response;

tokio::task_local! {
    /// Resolved originating client IP for the current request, if any.
    /// Read by `services::audit::write_audit_row` to populate
    /// `audit_log.ip_address`. Populated by `capture_client_ip_middleware`.
    pub static CLIENT_IP: Option<String>;
}

/// Networks whose source addresses are treated as "an internal proxy" and
/// therefore allowed to set X-Forwarded-For. Anything else is logged with
/// the peer IP directly, ignoring XFF.
fn is_trusted_proxy(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            // 127.0.0.0/8 — loopback
            if o[0] == 127 {
                return true;
            }
            // 10.0.0.0/8 — RFC1918
            if o[0] == 10 {
                return true;
            }
            // 172.16.0.0/12 — Docker default bridge networks live here
            if o[0] == 172 && (16..=31).contains(&o[1]) {
                return true;
            }
            // 192.168.0.0/16 — RFC1918, common Docker user-defined networks
            if o[0] == 192 && o[1] == 168 {
                return true;
            }
            false
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local(),
    }
}

/// Extract the first parseable IP from a comma-separated `X-Forwarded-For`
/// value. Returns `None` if the header is missing, empty, or the first
/// element is not a valid IP address (defensive — drop nonsense rather
/// than logging it).
fn first_xff_ip(headers: &HeaderMap) -> Option<IpAddr> {
    let raw = headers.get("x-forwarded-for")?.to_str().ok()?;
    let first = raw.split(',').next()?.trim();
    first.parse::<IpAddr>().ok()
}

/// Resolve the originating client IP. If the peer is on a trusted proxy
/// network and X-Forwarded-For has a parseable first entry, use that.
/// Otherwise fall back to the peer address.
pub fn resolve_client_ip(headers: &HeaderMap, peer: IpAddr) -> String {
    if is_trusted_proxy(peer) {
        if let Some(forwarded) = first_xff_ip(headers) {
            return forwarded.to_string();
        }
    }
    peer.to_string()
}

// NOTE: On Docker Desktop for Windows, the host NATs incoming connections
// to the Docker bridge gateway IP (e.g. 172.20.0.1) before they reach the
// nginx container. nginx then forwards that gateway IP in X-Forwarded-For,
// not the original client IP. As a result, audit_log.ip_address records
// 172.20.0.1 for all external clients on this deployment.
//
// On Linux Docker (production VPS), this NAT does not occur and real
// client IPs flow through correctly.
//
// EASA traceability is unaffected — user_id, user_code, action, and
// timestamp are accurate. Only per-client IP forensics are degraded.
/// Axum middleware that resolves the client IP and stores it in the
/// `CLIENT_IP` task-local for the lifetime of the handler.
pub async fn capture_client_ip_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let resolved = resolve_client_ip(request.headers(), addr.ip());
    CLIENT_IP.scope(Some(resolved), next.run(request)).await
}

/// Convenience accessor for code that runs inside a request scope.
/// Returns `None` outside of a request (e.g. background tasks).
pub fn current_client_ip() -> Option<String> {
    CLIENT_IP.try_with(|ip| ip.clone()).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;
    use std::net::Ipv4Addr;

    fn headers_with_xff(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", HeaderValue::from_str(value).unwrap());
        h
    }

    #[test]
    fn trusted_loopback_honours_xff() {
        let peer = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let h = headers_with_xff("203.0.113.42");
        assert_eq!(resolve_client_ip(&h, peer), "203.0.113.42");
    }

    #[test]
    fn trusted_docker_172_honours_xff() {
        let peer = IpAddr::V4(Ipv4Addr::new(172, 19, 0, 5));
        let h = headers_with_xff("198.51.100.7, 172.19.0.1");
        // Takes the first comma-separated entry.
        assert_eq!(resolve_client_ip(&h, peer), "198.51.100.7");
    }

    #[test]
    fn untrusted_public_peer_ignores_xff() {
        let peer = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));
        let h = headers_with_xff("203.0.113.42");
        assert_eq!(resolve_client_ip(&h, peer), "8.8.8.8");
    }

    #[test]
    fn missing_xff_falls_back_to_peer() {
        let peer = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let h = HeaderMap::new();
        assert_eq!(resolve_client_ip(&h, peer), "127.0.0.1");
    }

    #[test]
    fn unparseable_xff_falls_back_to_peer() {
        let peer = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let h = headers_with_xff("not-an-ip");
        assert_eq!(resolve_client_ip(&h, peer), "127.0.0.1");
    }

    #[test]
    fn trusted_192_168_honours_xff() {
        let peer = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
        let h = headers_with_xff("203.0.113.42");
        assert_eq!(resolve_client_ip(&h, peer), "203.0.113.42");
    }

    #[test]
    fn trusted_10_dot_honours_xff() {
        let peer = IpAddr::V4(Ipv4Addr::new(10, 0, 5, 7));
        let h = headers_with_xff("203.0.113.42");
        assert_eq!(resolve_client_ip(&h, peer), "203.0.113.42");
    }

    #[test]
    fn empty_xff_falls_back_to_peer() {
        let peer = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
        let h = headers_with_xff("");
        assert_eq!(resolve_client_ip(&h, peer), "127.0.0.1");
    }

    #[test]
    fn unique_local_v6_is_trusted() {
        // fc00::/7 (unique-local) is treated as proxy network.
        let peer: IpAddr = "fc00::1".parse().unwrap();
        let h = headers_with_xff("203.0.113.42");
        assert_eq!(resolve_client_ip(&h, peer), "203.0.113.42");
    }
}
