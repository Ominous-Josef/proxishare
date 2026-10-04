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

/// The local user's answer to an incoming pairing request, sent to the task
/// holding the pairing stream. `done` reports whether the pairing completed.
pub struct Decision {
    pub accepted: bool,
    pub done: oneshot::Sender<Result<(), String>>,
}

pub struct PendingIncoming {
    pub name: String,
    pub ip: String,
    pub port: u16,
    code: String,
    attempts: u8,
    expires: Instant,
    pub responder: oneshot::Sender<Decision>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairingError {
    NotFound,
    WrongCode { remaining: u8 },
    TooManyAttempts,
}

impl std::fmt::Display for PairingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PairingError::NotFound => write!(f, "No pending pairing request from this device (it may have expired)"),
            PairingError::WrongCode { remaining } => {
                write!(f, "Wrong pairing code ({} attempt(s) left)", remaining)
            }
            PairingError::TooManyAttempts => write!(f, "Too many wrong codes, pairing request discarded"),
        }
    }
}

#[derive(Default)]
pub struct PairingState {
    incoming: HashMap<String, PendingIncoming>,
    /// Pairings we started, so the user can cancel them.
    outgoing: HashMap<String, tokio::task::AbortHandle>,
}

impl PairingState {
    fn prune(&mut self) {
        let now = Instant::now();
        // Dropping an expired entry drops its responder, which ends the waiting task.
        self.incoming.retain(|_, p| p.expires > now);
        self.outgoing.retain(|_, handle| !handle.is_finished());
    }

    /// Stores an incoming request. An existing request for the same device is
    /// kept, so a second (possibly spoofed) request can't replace the real one.
    pub fn add_incoming(
        &mut self,
        device_id: &str,
        name: &str,
        ip: &str,
        port: u16,
        code: &str,
        responder: oneshot::Sender<Decision>,
    ) -> bool {
        self.prune();
        if self.incoming.contains_key(device_id) {
            return false;
        }
        self.incoming.insert(
            device_id.to_string(),
            PendingIncoming {
                name: name.to_string(),
                ip: ip.to_string(),
                port,
                code: code.to_string(),
                attempts: 0,
                expires: Instant::now() + PAIRING_TTL,
                responder,
            },
        );
        true
    }

    /// Checks the code the user typed. On success the request is removed and
    /// returned, so the caller can answer through its responder.
    pub fn verify_incoming(&mut self, device_id: &str, code: &str) -> Result<PendingIncoming, PairingError> {
        self.prune();
        let pending = self.incoming.get_mut(device_id).ok_or(PairingError::NotFound)?;
        if pending.code == code.trim() {
            return Ok(self.incoming.remove(device_id).unwrap());
        }
        pending.attempts += 1;
        if pending.attempts >= MAX_CODE_ATTEMPTS {
            self.incoming.remove(device_id);
            return Err(PairingError::TooManyAttempts);
        }
        Err(PairingError::WrongCode {
            remaining: MAX_CODE_ATTEMPTS - pending.attempts,
        })
    }

    pub fn remove_incoming(&mut self, device_id: &str) -> Option<PendingIncoming> {
        self.incoming.remove(device_id)
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

    fn add(p: &mut PairingState, id: &str, code: &str) -> oneshot::Receiver<Decision> {
        let (tx, rx) = oneshot::channel();
        assert!(p.add_incoming(id, "Laptop", "10.0.0.2", 51731, code, tx));
        rx
    }

    #[test]
    fn code_is_checked_with_limited_attempts() {
        let mut p = PairingState::default();
        let mut rx = add(&mut p, "dev", "123456");
        assert_eq!(p.verify_incoming("dev", "000000").err(), Some(PairingError::WrongCode { remaining: 2 }));
        assert_eq!(p.verify_incoming("dev", "111111").err(), Some(PairingError::WrongCode { remaining: 1 }));
        assert_eq!(p.verify_incoming("dev", "222222").err(), Some(PairingError::TooManyAttempts));
        assert_eq!(p.verify_incoming("dev", "123456").err(), Some(PairingError::NotFound));
        // The waiting stream learns the request is gone.
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn correct_code_hands_back_the_responder() {
        let mut p = PairingState::default();
        let mut rx = add(&mut p, "dev", "123456");
        let pending = p.verify_incoming("dev", " 123456 ").unwrap();
        assert_eq!(pending.ip, "10.0.0.2");
        let (done, _done_rx) = oneshot::channel();
        assert!(pending.responder.send(Decision { accepted: true, done }).is_ok());
        assert!(rx.try_recv().unwrap().accepted);
    }

    #[test]
    fn second_request_cannot_replace_pending_one() {
        let mut p = PairingState::default();
        let _rx = add(&mut p, "dev", "123456");
        let (tx, _rx2) = oneshot::channel();
        assert!(!p.add_incoming("dev", "Evil", "10.0.0.66", 51731, "999999", tx));
        assert_eq!(p.verify_incoming("dev", "123456").unwrap().ip, "10.0.0.2");
    }
}
