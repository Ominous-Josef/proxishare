//! Pending pairing handshakes, so that trust is only granted for pairings the
//! user actually started (outgoing) or confirmed with the right code (incoming).

use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const PAIRING_TTL: Duration = Duration::from_secs(300);
pub const MAX_CODE_ATTEMPTS: u8 = 3;

struct PendingOutgoing {
    ip: String,
    expires: Instant,
}

#[derive(Debug, Clone)]
pub struct PendingIncoming {
    pub name: String,
    pub ip: String,
    pub port: u16,
    code: String,
    attempts: u8,
    expires: Instant,
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
    outgoing: HashMap<String, PendingOutgoing>,
    incoming: HashMap<String, PendingIncoming>,
}

impl PairingState {
    fn prune(&mut self) {
        let now = Instant::now();
        self.outgoing.retain(|_, p| p.expires > now);
        self.incoming.retain(|_, p| p.expires > now);
    }

    /// Records that we asked `device_id` at `ip` to pair with us.
    pub fn expect_response(&mut self, device_id: &str, ip: &str) {
        self.prune();
        self.outgoing.insert(
            device_id.to_string(),
            PendingOutgoing {
                ip: ip.to_string(),
                expires: Instant::now() + PAIRING_TTL,
            },
        );
    }

    /// Consumes the pending outgoing request for `device_id`. Returns true only if
    /// we really asked this device to pair and the response comes from its address.
    pub fn take_response(&mut self, device_id: &str, from_ip: &str) -> bool {
        self.prune();
        match self.outgoing.get(device_id) {
            Some(p) if p.ip == from_ip => {
                self.outgoing.remove(device_id);
                true
            }
            _ => false,
        }
    }

    /// Stores an incoming request. An existing unexpired request for the same
    /// device is kept, so a spoofed request can't replace the real one.
    pub fn add_incoming(&mut self, device_id: &str, name: &str, ip: &str, port: u16, code: &str) -> bool {
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
            },
        );
        true
    }

    /// Checks the code the user typed. On success the request is consumed.
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsolicited_response_is_rejected() {
        let mut p = PairingState::default();
        assert!(!p.take_response("dev", "10.0.0.2"));
    }

    #[test]
    fn response_must_match_device_and_ip_and_is_single_use() {
        let mut p = PairingState::default();
        p.expect_response("dev", "10.0.0.2");
        assert!(!p.take_response("other", "10.0.0.2"));
        assert!(!p.take_response("dev", "10.0.0.66"));
        assert!(p.take_response("dev", "10.0.0.2"));
        assert!(!p.take_response("dev", "10.0.0.2"));
    }

    #[test]
    fn code_is_checked_with_limited_attempts() {
        let mut p = PairingState::default();
        assert!(p.add_incoming("dev", "Laptop", "10.0.0.2", 51731, "123456"));
        assert_eq!(p.verify_incoming("dev", "000000").unwrap_err(), PairingError::WrongCode { remaining: 2 });
        assert_eq!(p.verify_incoming("dev", "111111").unwrap_err(), PairingError::WrongCode { remaining: 1 });
        assert_eq!(p.verify_incoming("dev", "222222").unwrap_err(), PairingError::TooManyAttempts);
        assert_eq!(p.verify_incoming("dev", "123456").unwrap_err(), PairingError::NotFound);
    }

    #[test]
    fn correct_code_consumes_request() {
        let mut p = PairingState::default();
        p.add_incoming("dev", "Laptop", "10.0.0.2", 51731, "123456");
        let pending = p.verify_incoming("dev", "123456").unwrap();
        assert_eq!(pending.ip, "10.0.0.2");
        assert_eq!(p.verify_incoming("dev", "123456").unwrap_err(), PairingError::NotFound);
    }

    #[test]
    fn spoofed_request_cannot_replace_pending_one() {
        let mut p = PairingState::default();
        assert!(p.add_incoming("dev", "Laptop", "10.0.0.2", 51731, "123456"));
        assert!(!p.add_incoming("dev", "Evil", "10.0.0.66", 51731, "999999"));
        assert_eq!(p.verify_incoming("dev", "123456").unwrap().ip, "10.0.0.2");
    }
}
