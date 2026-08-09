use std::net::{IpAddr, SocketAddr};

use axum::{
    extract::ConnectInfo,
    http::{Extensions, HeaderMap},
};

/// Uses the proxy-provided client address only when the deployment explicitly
/// trusts its edge proxy. Render sets the first X-Forwarded-For entry to the
/// real client address; direct/local deployments fall back to the TCP peer.
pub(crate) fn client_ip(
    headers: &HeaderMap,
    extensions: &Extensions,
    trust_proxy: bool,
) -> Option<IpAddr> {
    if trust_proxy
        && let Some(forwarded) = headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse().ok())
    {
        return Some(forwarded);
    }

    extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwarded_address_is_only_used_for_a_trusted_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "203.0.113.7, 10.0.0.2".parse().unwrap());
        let mut extensions = Extensions::new();
        extensions.insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 3004))));

        assert_eq!(
            client_ip(&headers, &extensions, true),
            Some("203.0.113.7".parse().unwrap())
        );
        assert_eq!(
            client_ip(&headers, &extensions, false),
            Some("127.0.0.1".parse().unwrap())
        );
    }
}
