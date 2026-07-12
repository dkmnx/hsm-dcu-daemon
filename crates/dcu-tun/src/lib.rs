//! `dcu-tun` — Linux TUN IPv6 interface management for the DCU daemon.
//!
//! Port of `src/util/tunnel.c`, `TunnelIPv6Interface.*`, and
//! `netif-mgmt.c`. Provides TUN device allocation, IPv6 address/route
//! management, and synchronous + asynchronous packet read/write for the
//! daemon's event loop.

pub mod device;
pub mod error;
pub mod interface;
pub mod ioctl;
pub mod packet;

pub use device::{TunConfig, TunDevice};
pub use error::{Ipv6Net, TunError};
pub use interface::TunnelIPv6Interface;
pub use packet::{IPv6Header, get_ipv6_payload, is_ipv6_packet, parse_ipv6_header};

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    // -- Pure helper tests (no privileges needed) --

    #[test]
    fn prefix_len_from_netmask() {
        // /64: 64 ones
        let mask: Ipv6Addr = "ffff:ffff:ffff:ffff::".parse().unwrap();
        assert_eq!(crate::ioctl::prefix_len(mask), 64);

        // /128: all ones
        let mask: Ipv6Addr = "ffff:ffff:ffff:ffff:ffff:ffff:ffff:ffff".parse().unwrap();
        assert_eq!(crate::ioctl::prefix_len(mask), 128);

        // /0: zero
        let mask: Ipv6Addr = Ipv6Addr::UNSPECIFIED;
        assert_eq!(crate::ioctl::prefix_len(mask), 0);
    }

    // -- Existing tests --
    #[test]
    #[ignore = "requires /dev/net/tun and CAP_NET_ADMIN"]
    fn tun_device_lifecycle() {
        let config = TunConfig {
            name: "test_tun0".into(),
            mtu: 1280,
            no_packet_info: true,
        };
        let dev = TunDevice::open(config).unwrap();
        assert_eq!(dev.name(), "test_tun0");
        dev.close();
    }

    #[test]
    #[ignore = "requires an unprivileged network namespace (unshare -rn)"]
    fn tun_device_in_namespace() {
        // Opening a TUN inside a user namespace (e.g. `unshare -rn`) does not
        // require host root. Run with: cargo test -- --ignored
        let config = TunConfig::default();
        let dev = TunDevice::open(config).unwrap();
        assert!(!dev.name().is_empty());
        let _ = dev.is_up();
    }

    #[test]
    fn ipv6_address_format() {
        let addr: Ipv6Addr = "2020:abcd::212:4b00:14f7:d160".parse().unwrap();
        assert_eq!(
            addr.octets(),
            [
                0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0, 0x02, 0x12, 0x4B, 0x00, 0x14, 0xF7, 0xD1, 0x60
            ]
        );
    }

    #[test]
    fn mtu_bounds_check() {
        assert!(
            TunConfig {
                name: "t".into(),
                mtu: 1280,
                no_packet_info: true
            }
            .is_valid()
        );
        assert!(
            TunConfig {
                name: "t".into(),
                mtu: 1200,
                no_packet_info: true
            }
            .is_valid()
        );
        assert!(
            !TunConfig {
                name: "t".into(),
                mtu: 64,
                no_packet_info: true
            }
            .is_valid()
        );
        assert!(
            !TunConfig {
                name: "t".into(),
                mtu: 65535,
                no_packet_info: true
            }
            .is_valid()
        );
    }

    #[test]
    fn packet_round_trip_via_pipe() {
        // Exercise the packet parsing layer without a TUN device.
        let packet = build_test_packet();
        assert!(is_ipv6_packet(&packet));
        let payload = get_ipv6_payload(&packet);
        assert_eq!(payload, packet);

        let header = parse_ipv6_header(&packet).unwrap();
        assert_eq!(header.version, 6);
        assert_eq!(header.payload_length, 16);
        assert_eq!(header.next_header, 0x3A);
        assert_eq!(header.hop_limit, 64);
    }

    #[test]
    fn parse_ipv6_header_basic() {
        let packet = vec![
            0x60, 0x00, 0x00, 0x00, // version=6, traffic class, flow label
            0x00, 0x10, // payload length = 16
            0x3A, // next header = ICMPv6
            0x40, // hop limit = 64
            // source: 2020:abcd::1
            0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
            // dest: 2020:abcd::2
            0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ];
        let header = parse_ipv6_header(&packet).unwrap();
        assert_eq!(header.version, 6);
        assert_eq!(header.payload_length, 16);
        assert_eq!(header.next_header, 0x3A);
        assert_eq!(header.hop_limit, 64);
        assert_eq!(header.source, "2020:abcd::1".parse::<Ipv6Addr>().unwrap());
        assert_eq!(
            header.destination,
            "2020:abcd::2".parse::<Ipv6Addr>().unwrap()
        );
    }

    /// Build an IPv6 packet matching the spec Test 6 vector.
    fn build_test_packet() -> Vec<u8> {
        vec![
            0x60, 0x00, 0x00, 0x00, 0x00, 0x10, 0x3A, 0x40, 0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 1, 0x20, 0x20, 0xAB, 0xCD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ]
    }
}
