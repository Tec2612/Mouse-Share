use std::net::{IpAddr, SocketAddr};
use thiserror::Error;

/// A user-entered `host[:port]` target for connecting to a device that
/// mDNS didn't discover — common on networks with client (AP) isolation,
/// VPNs, or other multicast-hostile setups. Accepts a bare IPv4/IPv6
/// address, an IPv6 address in brackets, or a DNS hostname, each with an
/// optional `:port` suffix; falls back to `default_port` when omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualTarget {
    Ip(SocketAddr),
    Hostname { host: String, port: u16 },
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ManualTargetError {
    #[error("`{0}` is not a valid host[:port]")]
    InvalidFormat(String),
    #[error("port must be between 1 and 65535")]
    InvalidPort,
}

pub fn parse_manual_target(input: &str, default_port: u16) -> Result<ManualTarget, ManualTargetError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ManualTargetError::InvalidFormat(input.to_string()));
    }

    // Bracketed IPv6 with an explicit port: [::1]:12345
    if let Some(rest) = input.strip_prefix('[') {
        let (addr_part, after) = rest
            .split_once(']')
            .ok_or_else(|| ManualTargetError::InvalidFormat(input.to_string()))?;
        let ip: IpAddr = addr_part.parse().map_err(|_| ManualTargetError::InvalidFormat(input.to_string()))?;
        let port = match after.strip_prefix(':') {
            Some(p) => p.parse().map_err(|_| ManualTargetError::InvalidPort)?,
            None if after.is_empty() => default_port,
            None => return Err(ManualTargetError::InvalidFormat(input.to_string())),
        };
        return Ok(ManualTarget::Ip(SocketAddr::new(ip, port)));
    }

    // Bare IPv6 with no port (can't disambiguate a trailing `:NNNN` from
    // the address's own colons), e.g. "::1" or "fe80::1".
    if input.matches(':').count() > 1 {
        let ip: IpAddr = input.parse().map_err(|_| ManualTargetError::InvalidFormat(input.to_string()))?;
        return Ok(ManualTarget::Ip(SocketAddr::new(ip, default_port)));
    }

    // IPv4 or hostname, optionally with a single ":port" suffix.
    let (host, port) = match input.split_once(':') {
        Some((h, p)) => (h, p.parse().map_err(|_| ManualTargetError::InvalidPort)?),
        None => (input, default_port),
    };
    if port == 0 {
        return Err(ManualTargetError::InvalidPort);
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ManualTarget::Ip(SocketAddr::new(ip, port)));
    }

    if host.is_empty() {
        return Err(ManualTargetError::InvalidFormat(input.to_string()));
    }
    Ok(ManualTarget::Hostname { host: host.to_string(), port })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn bare_ipv4_uses_the_default_port() {
        let t = parse_manual_target("192.168.1.20", 45678).unwrap();
        assert_eq!(t, ManualTarget::Ip(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), 45678)));
    }

    #[test]
    fn ipv4_with_explicit_port() {
        let t = parse_manual_target("192.168.1.20:9000", 45678).unwrap();
        assert_eq!(t, ManualTarget::Ip(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20)), 9000)));
    }

    #[test]
    fn bracketed_ipv6_with_port() {
        let t = parse_manual_target("[::1]:9000", 45678).unwrap();
        assert_eq!(t, ManualTarget::Ip(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 9000)));
    }

    #[test]
    fn bare_ipv6_without_port_uses_default() {
        let t = parse_manual_target("::1", 45678).unwrap();
        assert_eq!(t, ManualTarget::Ip(SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 45678)));
    }

    #[test]
    fn hostname_with_and_without_port() {
        assert_eq!(
            parse_manual_target("bobs-mac.local", 45678).unwrap(),
            ManualTarget::Hostname { host: "bobs-mac.local".into(), port: 45678 }
        );
        assert_eq!(
            parse_manual_target("bobs-mac.local:9000", 45678).unwrap(),
            ManualTarget::Hostname { host: "bobs-mac.local".into(), port: 9000 }
        );
    }

    #[test]
    fn empty_input_is_rejected() {
        assert!(parse_manual_target("", 45678).is_err());
        assert!(parse_manual_target("   ", 45678).is_err());
    }

    #[test]
    fn zero_port_is_rejected() {
        assert_eq!(parse_manual_target("host:0", 45678), Err(ManualTargetError::InvalidPort));
    }

    #[test]
    fn garbage_port_is_rejected() {
        assert_eq!(parse_manual_target("host:notaport", 45678), Err(ManualTargetError::InvalidPort));
    }
}
