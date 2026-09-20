//! Hosted egress policy. A hosted tenant may only target public HTTPS Coolify
//! endpoints; local stdio clients intentionally retain the legacy `new` path.
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use url::Url;

pub fn validate_hosted_base_url(url: &Url) -> Result<(), String> {
    if url.scheme() != "https" {
        return Err("hosted Coolify URLs must use HTTPS".into());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "missing destination host".to_owned())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "missing destination port".to_owned())?;
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| "destination host could not be resolved".to_owned())?;
    let mut found = false;
    for address in addresses {
        found = true;
        if !is_public_address(address.ip()) {
            return Err(
                "hosted Coolify destination resolves to a private or reserved address".into(),
            );
        }
    }
    if !found {
        return Err("destination host has no addresses".into());
    }
    Ok(())
}

fn is_public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || ip.octets()[0] == 0
                || ip.octets()[0] >= 224
                || in_v4_range(ip, [100, 64, 0, 0], [100, 127, 255, 255])
                || in_v4_range(ip, [192, 0, 0, 0], [192, 0, 0, 255])
                || in_v4_range(ip, [192, 0, 2, 0], [192, 0, 2, 255])
                || in_v4_range(ip, [192, 88, 99, 0], [192, 88, 99, 255])
                || in_v4_range(ip, [198, 18, 0, 0], [198, 19, 255, 255])
                || in_v4_range(ip, [198, 51, 100, 0], [198, 51, 100, 255])
                || in_v4_range(ip, [203, 0, 113, 0], [203, 0, 113, 255]))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_address(IpAddr::V4(mapped));
            }
            let segments = ip.segments();
            !(ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00 // ULA
                || (segments[0] & 0xffc0) == 0xfe80 // link-local
                || (segments[0] == 0x2001 && segments[1] == 0x0db8) // documentation
                || (segments[0] == 0x2001 && segments[1] == 0x0002) // benchmarking
                || (segments[0] == 0x2001 && segments[1] == 0x0010) // ORCHID
                || segments[0] == 0x3fff) // documentation
        }
    }
}

fn in_v4_range(ip: Ipv4Addr, first: [u8; 4], last: [u8; 4]) -> bool {
    let value = u32::from(ip);
    value >= u32::from(Ipv4Addr::from(first)) && value <= u32::from(Ipv4Addr::from(last))
}

#[cfg(test)]
mod tests {
    use super::is_public_address;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn metadata_and_reserved_ranges_are_not_public() {
        for ip in [
            IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
            IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::from_segments([0xfc00, 0, 0, 0, 0, 0, 0, 1])),
        ] {
            assert!(!is_public_address(ip));
        }
    }
}
