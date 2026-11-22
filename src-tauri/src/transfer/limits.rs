//! Caps on what peers can hold open, so nobody can exhaust our memory by
//! opening connections. Paired devices get generous limits (several files
//! can be sent at once, each on its own connection); unknown keys only ever
//! need a ping or a pairing request, so they get very few.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

/// Handshakes we run at once. Further connection attempts are refused until
/// one finishes, which bounds the CPU an unauthenticated flood can use.
pub const MAX_HANDSHAKES: usize = 64;

/// Connection attempts quinn may queue before we look at them.
pub const MAX_QUEUED_INCOMING: usize = 256;

/// The address a limit is counted against. A single machine can hold any
/// number of IPv6 addresses within its /64, so IPv6 is limited per /64.
pub fn address_group(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => {
            let mut segments = v6.segments();
            segments[4..].fill(0);
            IpAddr::V6(segments.into())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PeerClass {
    /// Holds the key of a paired device.
    Paired,
    /// Any other key.
    Unknown,
}

impl PeerClass {
    fn limits(self) -> (usize, usize) {
        match self {
            PeerClass::Paired => (MAX_PAIRED_PER_ADDRESS, MAX_PAIRED_TOTAL),
            PeerClass::Unknown => (MAX_UNKNOWN_PER_ADDRESS, MAX_UNKNOWN_TOTAL),
        }
    }
}

const MAX_PAIRED_PER_ADDRESS: usize = 32;
const MAX_PAIRED_TOTAL: usize = 128;
const MAX_UNKNOWN_PER_ADDRESS: usize = 4;
const MAX_UNKNOWN_TOTAL: usize = 32;

#[derive(Default)]
struct Counts {
    per_address: HashMap<(IpAddr, PeerClass), usize>,
    total: HashMap<PeerClass, usize>,
}

/// Open connections, counted per address group and per class.
#[derive(Default)]
pub struct ConnectionLimiter {
    counts: Mutex<Counts>,
}

/// One counted connection. Dropping it frees the slot.
pub struct ConnectionSlot {
    limiter: Arc<ConnectionLimiter>,
    key: (IpAddr, PeerClass),
}

impl ConnectionLimiter {
    /// Takes a slot for a connection from `ip`, or `None` if that address or
    /// the class as a whole already has too many open.
    pub fn try_acquire(self: &Arc<Self>, ip: IpAddr, class: PeerClass) -> Option<ConnectionSlot> {
        let (per_address_max, total_max) = class.limits();
        let key = (address_group(ip), class);
        let mut counts = self.counts.lock();
        let at_address = counts.per_address.get(&key).copied().unwrap_or(0);
        let total = counts.total.get(&class).copied().unwrap_or(0);
        if at_address >= per_address_max || total >= total_max {
            return None;
        }
        counts.per_address.insert(key, at_address + 1);
        counts.total.insert(class, total + 1);
        Some(ConnectionSlot {
            limiter: self.clone(),
            key,
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        let mut counts = self.limiter.counts.lock();
        if let Some(n) = counts.per_address.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                counts.per_address.remove(&self.key);
            }
        }
        if let Some(n) = counts.total.get_mut(&self.key.1) {
            *n -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn unknown_keys_are_capped_per_address() {
        let limiter = Arc::new(ConnectionLimiter::default());
        let held: Vec<_> = (0..MAX_UNKNOWN_PER_ADDRESS)
            .map(|_| limiter.try_acquire(ip("10.0.0.9"), PeerClass::Unknown).unwrap())
            .collect();
        assert!(limiter.try_acquire(ip("10.0.0.9"), PeerClass::Unknown).is_none());
        // Other addresses, and paired devices at the same address, are unaffected.
        assert!(limiter.try_acquire(ip("10.0.0.10"), PeerClass::Unknown).is_some());
        assert!(limiter.try_acquire(ip("10.0.0.9"), PeerClass::Paired).is_some());
        drop(held);
        assert!(limiter.try_acquire(ip("10.0.0.9"), PeerClass::Unknown).is_some());
    }

    #[test]
    fn unknown_keys_are_capped_in_total() {
        let limiter = Arc::new(ConnectionLimiter::default());
        let held: Vec<_> = (0..MAX_UNKNOWN_TOTAL)
            .map(|i| {
                limiter
                    .try_acquire(ip(&format!("10.0.1.{}", i)), PeerClass::Unknown)
                    .unwrap()
            })
            .collect();
        assert!(limiter.try_acquire(ip("10.0.2.1"), PeerClass::Unknown).is_none());
        assert!(limiter.try_acquire(ip("10.0.2.1"), PeerClass::Paired).is_some());
        drop(held);
    }

    #[test]
    fn paired_devices_can_send_several_files_at_once() {
        let limiter = Arc::new(ConnectionLimiter::default());
        let held: Vec<_> = (0..20)
            .map(|_| limiter.try_acquire(ip("10.0.0.2"), PeerClass::Paired))
            .collect();
        assert!(held.iter().all(Option::is_some));
    }

    #[test]
    fn ipv6_is_limited_per_slash_64() {
        assert_eq!(
            address_group(ip("2001:db8:1:2:aaaa:bbbb:cccc:dddd")),
            address_group(ip("2001:db8:1:2::1"))
        );
        assert_ne!(
            address_group(ip("2001:db8:1:2::1")),
            address_group(ip("2001:db8:1:3::1"))
        );
        assert_eq!(address_group(ip("10.0.0.1")), ip("10.0.0.1"));
    }
}
