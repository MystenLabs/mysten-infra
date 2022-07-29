// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use multiaddr::{Multiaddr, Protocol};
use std::{
    borrow::Cow,
    net::{IpAddr, SocketAddr},
};

// Converts a /ip{4,6}/-/tcp/-[/-] Multiaddr to SocketAddr.
pub fn to_socket_addr(addr: &Multiaddr) -> Result<SocketAddr> {
    let mut iter = addr.iter();
    let ip = match iter
        .next()
        .ok_or_else(|| anyhow!("failed to convert to SocketAddr: Multiaddr does not contain IP"))?
    {
        Protocol::Ip4(ip4_addr) => IpAddr::V4(ip4_addr),
        Protocol::Ip6(ip6_addr) => IpAddr::V6(ip6_addr),
        unsupported => return Err(anyhow!("unsupported protocol {unsupported}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    Ok(SocketAddr::new(ip, tcp_port))
}

pub(crate) fn parse_tcp<'a, T: Iterator<Item = Protocol<'a>>>(protocols: &mut T) -> Result<u16> {
    if let Protocol::Tcp(port) = protocols
        .next()
        .ok_or_else(|| anyhow!("unexpected end of multiaddr"))?
    {
        Ok(port)
    } else {
        Err(anyhow!("expected tcp protocol"))
    }
}

pub(crate) fn parse_http_https<'a, T: Iterator<Item = Protocol<'a>>>(
    protocols: &mut T,
) -> Result<&'static str> {
    match protocols
        .next()
        .ok_or_else(|| anyhow!("unexpected end of multiaddr"))?
    {
        Protocol::Http => Ok("http"),
        Protocol::Https => Ok("https"),
        _ => Err(anyhow!("expected http/https protocol")),
    }
}

pub(crate) fn parse_end<'a, T: Iterator<Item = Protocol<'a>>>(protocols: &mut T) -> Result<()> {
    if protocols.next().is_none() {
        Ok(())
    } else {
        Err(anyhow!("expected end of multiaddr"))
    }
}

// Parse a full /dns/-/tcp/-/{http,https} address
pub(crate) fn parse_dns(address: &Multiaddr) -> Result<(Cow<'_, str>, u16, &'static str)> {
    let mut iter = address.iter();

    let dns_name = match iter
        .next()
        .ok_or_else(|| anyhow!("unexpected end of multiaddr"))?
    {
        Protocol::Dns(dns_name) => dns_name,
        other => return Err(anyhow!("expected dns found {other}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;
    Ok((dns_name, tcp_port, http_or_https))
}

// Parse a full /ip4/-/tcp/-/{http,https} address
pub(crate) fn parse_ip4(address: &Multiaddr) -> Result<(SocketAddr, &'static str)> {
    let mut iter = address.iter();

    let ip_addr = match iter
        .next()
        .ok_or_else(|| anyhow!("unexpected end of multiaddr"))?
    {
        Protocol::Ip4(ip4_addr) => IpAddr::V4(ip4_addr),
        other => return Err(anyhow!("expected ip4 found {other}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;
    let socket_addr = SocketAddr::new(ip_addr, tcp_port);

    Ok((socket_addr, http_or_https))
}

// Parse a full /ip6/-/tcp/-/{http,https} address
pub(crate) fn parse_ip6(address: &Multiaddr) -> Result<(SocketAddr, &'static str)> {
    let mut iter = address.iter();

    let ip_addr = match iter
        .next()
        .ok_or_else(|| anyhow!("unexpected end of multiaddr"))?
    {
        Protocol::Ip6(ip6_addr) => IpAddr::V6(ip6_addr),
        other => return Err(anyhow!("expected ip6 found {other}")),
    };
    let tcp_port = parse_tcp(&mut iter)?;
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;
    let socket_addr = SocketAddr::new(ip_addr, tcp_port);

    Ok((socket_addr, http_or_https))
}

// Parse a full /unix/-/{http,https} address
#[cfg(unix)]
pub(crate) fn parse_unix(address: &Multiaddr) -> Result<(Cow<'_, str>, &'static str)> {
    let mut iter = address.iter();

    let path = match iter
        .next()
        .ok_or_else(|| anyhow!("unexpected end of multiaddr"))?
    {
        Protocol::Unix(path) => path,
        other => return Err(anyhow!("expected unix found {other}")),
    };
    let http_or_https = parse_http_https(&mut iter)?;
    parse_end(&mut iter)?;

    Ok((path, http_or_https))
}

#[cfg(test)]
mod test {
    use super::to_socket_addr;
    use multiaddr::multiaddr;

    #[test]
    fn test_to_socket_addr() {
        let multi_addr_ipv4 = multiaddr!(Ip4([127, 0, 0, 1]), Tcp(10500u16));
        let socket_addr_ipv4 =
            to_socket_addr(&multi_addr_ipv4).expect("Couldn't convert to socket addr");
        assert_eq!(socket_addr_ipv4.to_string(), "127.0.0.1:10500");

        let multi_addr_ipv6 = multiaddr!(Ip6([172, 0, 0, 1, 1, 1, 1, 1]), Tcp(10500u16));
        let socket_addr_ipv6 =
            to_socket_addr(&multi_addr_ipv6).expect("Couldn't convert to socket addr");
        assert_eq!(socket_addr_ipv6.to_string(), "[ac::1:1:1:1:1]:10500");
    }
}
