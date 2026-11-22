//! Key-bound pairing.
//!
//! The pairing code both users see is derived from both devices' key
//! fingerprints and IDs plus one fresh nonce from each side. The initiator
//! commits to its nonce before seeing the responder's, so nobody (including a
//! man-in-the-middle holding its own keys) can choose nonces to make the two
//! codes match. A MITM ends up with a different fingerprint pair on each leg,
//! and therefore different codes, except with probability ~1e-6 per attempt.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;

pub const PAIRING_TTL: Duration = Duration::from_secs(300);
pub const MAX_CODE_ATTEMPTS: u8 = 3;
/// Open requests one address may hold, across all device IDs. Keys are free
/// to make, so this, not the per-key rule, is what limits a single attacker.
const MAX_PENDING_PER_ADDRESS: usize = 3;
/// Open requests overall.
const MAX_PENDING_TOTAL: usize = 64;

/// What `add_incoming` did with a request.
#[derive(Debug, PartialEq, Eq)]
pub enum Added {
    /// The device's first open request: the UI should be asked.
    First,
    /// Joined requests already open for this device ID (the UI is already asking).
    Joined,
    /// Too many open requests from this address, or overall.
    Refused,
}

pub type Nonce = [u8; 32];

pub fn new_nonce() -> Nonce {
    use rand::RngCore;
    let mut nonce = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut nonce);
    nonce
}

pub fn commitment(nonce: &Nonce) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new_derive_key("proxishare pairing commitment v1");
    hasher.update(nonce);
    *hasher.finalize().as_bytes()
}

/// One side of a pairing: the ID it claims and the key it proved it holds.
pub struct PairParty<'a> {
    pub device_id: &'a str,
    pub fingerprint: &'a str,
}

/// The 6-digit code. Both sides compute it with the same argument order:
/// initiator first, responder second.
pub fn pairing_code(
    initiator: &PairParty,
    responder: &PairParty,
    initiator_nonce: &Nonce,
    responder_nonce: &Nonce,
) -> String {
    let mut hasher = blake3::Hasher::new_derive_key("proxishare pairing code v1");
    for field in [
        initiator.device_id.as_bytes(),
        initiator.fingerprint.as_bytes(),
        responder.device_id.as_bytes(),
        responder.fingerprint.as_bytes(),
    ] {
        hasher.update(&(field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    hasher.update(initiator_nonce);
    hasher.update(responder_nonce);

    let digest = hasher.finalize();
    let mut first = [0u8; 8];
    first.copy_from_slice(&digest.as_bytes()[..8]);
    format!("{:06}", u64::from_be_bytes(first) % 1_000_000)
}

/// The answer to an incoming pairing request, sent to the task holding its
/// pairing stream.
pub enum Decision {
    /// Our user typed this request's code. `done` reports whether pairing completed.
    Accept {
        done: oneshot::Sender<Result<(), String>>,
    },
    /// Our user declined.
    Decline,
    /// Another request for the same device took over: a retry from the same
    /// key, or the request whose code matched. Ends without telling the UI,
    /// which is still busy with the other request.
    Superseded,
}

pub struct PendingIncoming {
    pub name: String,
    pub ip: String,
    pub port: u16,
    /// The key the requester proved it holds.
    fingerprint: String,
    /// The address the request is counted against.
    address: String,
    code: String,
    expires: Instant,
    pub responder: oneshot::Sender<Decision>,
}

/// The open requests claiming one device ID. Anyone can claim any ID, so
/// requests are told apart by key, and only the one whose code our user types
/// is accepted. Wrong codes count against the device, so a new request can't
/// reset the attempts.
#[derive(Default)]
struct PendingDevice {
    requests: Vec<PendingIncoming>,
    attempts: u8,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairingError {
    NotFound,
    WrongCode { remaining: u8 },
    TooManyAttempts,
    /// Two open requests have the same code (about one in a million).
    Ambiguous,
}

impl std::fmt::Display for PairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PairingError::NotFound => write!(f, "No pending pairing request from this device (it may have expired)"),
            PairingError::WrongCode { remaining } => {
                write!(f, "Wrong pairing code ({} attempt(s) left)", remaining)
            }
            PairingError::TooManyAttempts => write!(f, "Too many wrong codes, pairing request discarded"),
            PairingError::Ambiguous => write!(f, "Pairing could not be confirmed. Ask the other device to try again"),
        }
    }
}

#[derive(Default)]
pub struct PairingState {
    incoming: HashMap<String, PendingDevice>,
    /// Pairings we started, so the user can cancel them.
    outgoing: HashMap<String, tokio::task::AbortHandle>,
}

impl PairingState {
    fn prune(&mut self) {
        let now = Instant::now();
        // Dropping an expired request drops its responder, which ends the waiting task.
        for device in self.incoming.values_mut() {
            device.requests.retain(|p| p.expires > now);
        }
        self.incoming.retain(|_, device| !device.requests.is_empty());
        self.outgoing.retain(|_, handle| !handle.is_finished());
    }

    /// Stores an incoming request. A request from a different key sits beside
    /// any already open for this device ID, so a spoofed request can neither
    /// replace nor block the real one; a request from the same key replaces
    /// its own earlier one. Each address may only hold a few open requests.
    #[allow(clippy::too_many_arguments)]
    pub fn add_incoming(
        &mut self,
        device_id: &str,
        fingerprint: &str,
        name: &str,
        ip: &str,
        port: u16,
        code: &str,
        responder: oneshot::Sender<Decision>,
    ) -> Added {
        self.prune();
        let address = match ip.parse() {
            Ok(ip) => crate::transfer::limits::address_group(ip).to_string(),
            Err(_) => ip.to_string(),
        };
        // A retry from the same key replaces its own request, so it doesn't count.
        let replaces = |p: &PendingIncoming| p.fingerprint == fingerprint;
        let replacing = self
            .incoming
            .get(device_id)
            .is_some_and(|d| d.requests.iter().any(replaces));
        let others = self.incoming.iter().flat_map(|(id, d)| {
            d.requests
                .iter()
                .filter(move |p| !(id == device_id && replaces(p)))
        });
        let (mut total, mut at_address) = (0, 0);
        for p in others {
            total += 1;
            if p.address == address {
                at_address += 1;
            }
        }
        if !replacing && (at_address >= MAX_PENDING_PER_ADDRESS || total >= MAX_PENDING_TOTAL) {
            return Added::Refused;
        }

        let device = self.incoming.entry(device_id.to_string()).or_default();
        let first = device.requests.is_empty();
        if let Some(i) = device.requests.iter().position(replaces) {
            let _ = device.requests.swap_remove(i).responder.send(Decision::Superseded);
        }
        device.requests.push(PendingIncoming {
            name: name.to_string(),
            ip: ip.to_string(),
            port,
            fingerprint: fingerprint.to_string(),
            address,
            code: code.to_string(),
            expires: Instant::now() + PAIRING_TTL,
            responder,
        });
        if first {
            Added::First
        } else {
            Added::Joined
        }
    }

    /// Checks the code the user typed against the device's open requests. On
    /// a match that request is removed and returned, so the caller can answer
    /// through its responder; the device's other requests are superseded.
    pub fn verify_incoming(&mut self, device_id: &str, code: &str) -> Result<PendingIncoming, PairingError> {
        self.prune();
        let device = self.incoming.get_mut(device_id).ok_or(PairingError::NotFound)?;
        let code = code.trim();
        let matches: Vec<usize> = (0..device.requests.len())
            .filter(|&i| device.requests[i].code == code)
            .collect();

        match matches[..] {
            [i] => {
                let mut device = self.incoming.remove(device_id).unwrap();
                let chosen = device.requests.swap_remove(i);
                for rival in device.requests {
                    let _ = rival.responder.send(Decision::Superseded);
                }
                Ok(chosen)
            }
            [] => {
                device.attempts += 1;
                if device.attempts >= MAX_CODE_ATTEMPTS {
                    self.incoming.remove(device_id);
                    return Err(PairingError::TooManyAttempts);
                }
                Err(PairingError::WrongCode {
                    remaining: MAX_CODE_ATTEMPTS - device.attempts,
                })
            }
            _ => {
                self.incoming.remove(device_id);
                Err(PairingError::Ambiguous)
            }
        }
    }

    /// Removes the request from this device ID and key, e.g. when it gives up.
    pub fn remove_incoming(&mut self, device_id: &str, fingerprint: &str) -> Option<PendingIncoming> {
        let device = self.incoming.get_mut(device_id)?;
        let i = device.requests.iter().position(|p| p.fingerprint == fingerprint)?;
        let removed = device.requests.swap_remove(i);
        if device.requests.is_empty() {
            self.incoming.remove(device_id);
        }
        Some(removed)
    }

    /// Removes every request for this device ID (the user declined).
    pub fn remove_all_incoming(&mut self, device_id: &str) -> Vec<PendingIncoming> {
        self.incoming
            .remove(device_id)
            .map(|device| device.requests)
            .unwrap_or_default()
    }

    /// Whether any request for this device ID is still open.
    pub fn has_incoming(&mut self, device_id: &str) -> bool {
        self.prune();
        self.incoming.contains_key(device_id)
    }

    /// Registers a pairing we started; replaces (and cancels) an older one.
    pub fn add_outgoing(&mut self, device_id: &str, handle: tokio::task::AbortHandle) {
        self.prune();
        if let Some(old) = self.outgoing.insert(device_id.to_string(), handle) {
            old.abort();
        }
    }

    /// Cancels a pairing we started. Dropping its connection tells the other side.
    pub fn cancel_outgoing(&mut self, device_id: &str) {
        if let Some(handle) = self.outgoing.remove(device_id) {
            handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn party<'a>(id: &'a str, fp: &'a str) -> PairParty<'a> {
        PairParty {
            device_id: id,
            fingerprint: fp,
        }
    }

    #[test]
    fn code_is_deterministic_and_six_digits() {
        let (na, nb) = ([1u8; 32], [2u8; 32]);
        let a = pairing_code(&party("a", "fa"), &party("b", "fb"), &na, &nb);
        let b = pairing_code(&party("a", "fa"), &party("b", "fb"), &na, &nb);
        assert_eq!(a, b);
        assert_eq!(a.len(), 6);
        assert!(a.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn code_changes_with_any_key_or_nonce() {
        let (na, nb) = ([1u8; 32], [2u8; 32]);
        let base = pairing_code(&party("a", "fa"), &party("b", "fb"), &na, &nb);
        // A man-in-the-middle presents its own key on one leg.
        assert_ne!(base, pairing_code(&party("a", "fa"), &party("b", "fm"), &na, &nb));
        assert_ne!(base, pairing_code(&party("a", "fm"), &party("b", "fb"), &na, &nb));
        assert_ne!(base, pairing_code(&party("a", "fa"), &party("b", "fb"), &[3u8; 32], &nb));
        assert_ne!(base, pairing_code(&party("a", "fa"), &party("b", "fb"), &na, &[3u8; 32]));
        // Swapping roles is a different pairing.
        assert_ne!(base, pairing_code(&party("b", "fb"), &party("a", "fa"), &nb, &na));
        // Length prefixes keep field boundaries unambiguous.
        assert_ne!(
            pairing_code(&party("ab", "c"), &party("x", "y"), &na, &nb),
            pairing_code(&party("a", "bc"), &party("x", "y"), &na, &nb)
        );
    }

    #[test]
    fn commitment_binds_the_nonce() {
        let n = new_nonce();
        assert_eq!(commitment(&n), commitment(&n));
        assert_ne!(commitment(&n), commitment(&new_nonce()));
    }

    fn add(p: &mut PairingState, id: &str, fp: &str, code: &str) -> (bool, oneshot::Receiver<Decision>) {
        add_from(p, "10.0.0.2", id, fp, code)
    }

    fn add_from(
        p: &mut PairingState,
        ip: &str,
        id: &str,
        fp: &str,
        code: &str,
    ) -> (bool, oneshot::Receiver<Decision>) {
        let (tx, rx) = oneshot::channel();
        let added = p.add_incoming(id, fp, "Laptop", ip, 51731, code, tx);
        assert_ne!(added, Added::Refused);
        (added == Added::First, rx)
    }

    #[test]
    fn one_address_cannot_hold_many_requests() {
        let mut p = PairingState::default();
        for i in 0..MAX_PENDING_PER_ADDRESS {
            add_from(&mut p, "10.0.0.66", &format!("spam-{}", i), &format!("k{}", i), "000000");
        }
        let (tx, _rx) = oneshot::channel();
        assert_eq!(p.add_incoming("spam-x", "kx", "Evil", "10.0.0.66", 51731, "000000", tx), Added::Refused);
        // A retry from a key already holding a request is still allowed.
        let (tx, _rx) = oneshot::channel();
        assert_ne!(p.add_incoming("spam-0", "k0", "Evil", "10.0.0.66", 51731, "000000", tx), Added::Refused);
        // The real device, elsewhere, is unaffected.
        let (first, _rx) = add_from(&mut p, "10.0.0.2", "dev", "fa", "123456");
        assert!(first);
    }

    #[test]
    fn ipv6_requests_are_counted_per_slash_64() {
        let mut p = PairingState::default();
        for i in 0..MAX_PENDING_PER_ADDRESS {
            add_from(&mut p, &format!("2001:db8::{}", i + 1), &format!("spam-{}", i), &format!("k{}", i), "000000");
        }
        let (tx, _rx) = oneshot::channel();
        assert_eq!(p.add_incoming("spam-x", "kx", "Evil", "2001:db8::ff", 51731, "000000", tx), Added::Refused);
    }

    fn superseded(rx: &mut oneshot::Receiver<Decision>) -> bool {
        matches!(rx.try_recv(), Ok(Decision::Superseded))
    }

    #[test]
    fn code_is_checked_with_limited_attempts() {
        let mut p = PairingState::default();
        let (_, mut rx) = add(&mut p, "dev", "fa", "123456");
        assert_eq!(p.verify_incoming("dev", "000000").err(), Some(PairingError::WrongCode { remaining: 2 }));
        assert_eq!(p.verify_incoming("dev", "111111").err(), Some(PairingError::WrongCode { remaining: 1 }));
        assert_eq!(p.verify_incoming("dev", "222222").err(), Some(PairingError::TooManyAttempts));
        assert_eq!(p.verify_incoming("dev", "123456").err(), Some(PairingError::NotFound));
        // The waiting stream learns the request is gone.
        assert!(rx.try_recv().is_err());
        assert!(!p.has_incoming("dev"));
    }

    #[test]
    fn correct_code_hands_back_the_responder() {
        let mut p = PairingState::default();
        let (first, mut rx) = add(&mut p, "dev", "fa", "123456");
        assert!(first);
        let pending = p.verify_incoming("dev", " 123456 ").unwrap();
        let (done, _done_rx) = oneshot::channel();
        assert!(pending.responder.send(Decision::Accept { done }).is_ok());
        assert!(matches!(rx.try_recv(), Ok(Decision::Accept { .. })));
    }

    #[test]
    fn spoofed_request_cannot_block_or_replace_the_real_one() {
        let mut p = PairingState::default();
        // The impostor gets in first, claiming the real device's ID with its own key.
        let (first, mut spoof_rx) = add(&mut p, "dev", "evil", "999999");
        assert!(first);
        let (first, _real_rx) = add(&mut p, "dev", "fa", "123456");
        assert!(!first, "the UI is already asking about this device");

        // The user types the code shown on the real device.
        let pending = p.verify_incoming("dev", "123456").unwrap();
        assert_eq!(pending.fingerprint, "fa");
        assert!(superseded(&mut spoof_rx));
        assert!(!p.has_incoming("dev"));
    }

    #[test]
    fn retry_from_the_same_key_replaces_its_own_request() {
        let mut p = PairingState::default();
        let (_, mut old_rx) = add(&mut p, "dev", "fa", "111111");
        let (first, _new_rx) = add(&mut p, "dev", "fa", "222222");
        assert!(!first);
        assert!(superseded(&mut old_rx));
        assert_eq!(p.verify_incoming("dev", "111111").err(), Some(PairingError::WrongCode { remaining: 2 }));
        assert_eq!(p.verify_incoming("dev", "222222").unwrap().fingerprint, "fa");
    }

    #[test]
    fn wrong_codes_count_per_device_not_per_request() {
        let mut p = PairingState::default();
        add(&mut p, "dev", "fa", "123456");
        assert!(p.verify_incoming("dev", "000000").is_err());
        assert!(p.verify_incoming("dev", "000001").is_err());
        // A fresh request doesn't buy more guesses.
        add(&mut p, "dev", "fb", "654321");
        assert_eq!(p.verify_incoming("dev", "000002").err(), Some(PairingError::TooManyAttempts));
    }

    #[test]
    fn clashing_codes_are_refused() {
        let mut p = PairingState::default();
        add(&mut p, "dev", "fa", "123456");
        add(&mut p, "dev", "fb", "123456");
        assert_eq!(p.verify_incoming("dev", "123456").err(), Some(PairingError::Ambiguous));
        assert!(!p.has_incoming("dev"));
    }

    #[test]
    fn a_request_giving_up_leaves_the_others_open() {
        let mut p = PairingState::default();
        add(&mut p, "dev", "fa", "123456");
        add(&mut p, "dev", "evil", "999999");
        assert!(p.remove_incoming("dev", "evil").is_some());
        assert!(p.has_incoming("dev"));
        assert!(p.remove_incoming("dev", "evil").is_none());
        assert_eq!(p.remove_all_incoming("dev").len(), 1);
        assert!(!p.has_incoming("dev"));
    }
}
