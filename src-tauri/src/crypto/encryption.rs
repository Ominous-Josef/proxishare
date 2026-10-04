//! Device identity and TLS configuration.
//!
//! Every device owns one long-lived Ed25519 key. There is no certificate
//! authority: a certificate is just a container for that key, and a device is
//! identified by the fingerprint of its public key. Both sides of every QUIC
//! connection present a certificate, and the handshake signatures are verified,
//! so `peer_fingerprint` reliably tells us which key is on the other end.
//! Whether that key belongs to a paired device is decided by the trust store.

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use std::path::Path;
use std::sync::Arc;

const IDENTITY_FILE: &str = "identity.key";

pub struct DeviceIdentity {
    cert_der: Vec<u8>,
    key_der: Vec<u8>,
    /// Fingerprint of this device's public key, as peers will see it.
    pub fingerprint: String,
}

impl DeviceIdentity {
    /// Loads the device key from `dir`, creating it on first launch. The
    /// certificate is rebuilt from the key each time; the fingerprint only
    /// depends on the key, so it never changes.
    pub fn load_or_create(dir: &Path) -> Result<Self, crate::GenericError> {
        let path = dir.join(IDENTITY_FILE);
        let key_pair = match std::fs::read(&path) {
            Ok(bytes) => rcgen::KeyPair::from_der(&bytes)
                .map_err(|e| format!("Device key at {:?} is unreadable: {}", path, e))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let key_pair = rcgen::KeyPair::generate(&rcgen::PKCS_ED25519)?;
                write_private_file(&path, &key_pair.serialize_der())?;
                println!("[Identity] Generated a new device key at {:?}", path);
                key_pair
            }
            Err(e) => return Err(e.into()),
        };
        Self::from_key_pair(key_pair)
    }

    /// A throwaway identity (tests and tooling).
    pub fn generate_ephemeral() -> Result<Self, crate::GenericError> {
        Self::from_key_pair(rcgen::KeyPair::generate(&rcgen::PKCS_ED25519)?)
    }

    fn from_key_pair(key_pair: rcgen::KeyPair) -> Result<Self, crate::GenericError> {
        let mut params = rcgen::CertificateParams::new(vec!["proxishare.local".to_string()]);
        params.alg = &rcgen::PKCS_ED25519;
        params.key_pair = Some(key_pair);
        let cert = rcgen::Certificate::from_params(params)?;

        let cert_der = cert.serialize_der()?;
        let key_der = cert.serialize_private_key_der();
        let fingerprint = fingerprint_of(&cert_der)?;
        Ok(Self {
            cert_der,
            key_der,
            fingerprint,
        })
    }

    fn cert_chain(&self) -> Vec<CertificateDer<'static>> {
        vec![CertificateDer::from(self.cert_der.clone())]
    }

    fn private_key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone()))
    }

    pub fn get_server_config(&self) -> Result<rustls::ServerConfig, crate::GenericError> {
        let mut config = rustls::ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .with_client_cert_verifier(Arc::new(PeerKeyVerifier::new()))
            .with_single_cert(self.cert_chain(), self.private_key())?;

        config.alpn_protocols = vec![b"proxishare".to_vec()];
        Ok(config)
    }

    pub fn get_client_config(&self) -> Result<rustls::ClientConfig, crate::GenericError> {
        let mut config = rustls::ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&rustls::version::TLS13])?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PeerKeyVerifier::new()))
            .with_client_auth_cert(self.cert_chain(), self.private_key())?;

        config.alpn_protocols = vec![b"proxishare".to_vec()];
        Ok(config)
    }
}

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Writes the private key so only the current user can read it.
fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Fingerprint of the public key inside a DER certificate.
pub fn fingerprint_of(cert_der: &[u8]) -> Result<String, rustls::Error> {
    let der = CertificateDer::from(cert_der);
    let parsed = rustls::server::ParsedCertificate::try_from(&der)?;
    Ok(blake3::hash(parsed.subject_public_key_info().as_ref())
        .to_hex()
        .to_string())
}

/// Fingerprint of the key the peer proved it holds during the handshake.
pub fn peer_fingerprint(connection: &quinn::Connection) -> Result<String, crate::GenericError> {
    let identity = connection
        .peer_identity()
        .ok_or("Peer presented no certificate")?;
    let certs = identity
        .downcast::<Vec<CertificateDer<'static>>>()
        .map_err(|_| "Unexpected peer identity type")?;
    let cert = certs.first().ok_or("Peer presented an empty certificate chain")?;
    Ok(fingerprint_of(cert)?)
}

/// Accepts any self-signed certificate (there is no CA), but verifies the
/// handshake signature, which proves the peer holds the certificate's key.
/// Which keys are acceptable is decided after the handshake, by fingerprint.
#[derive(Debug)]
struct PeerKeyVerifier {
    algorithms: WebPkiSupportedAlgorithms,
}

impl PeerKeyVerifier {
    fn new() -> Self {
        Self {
            algorithms: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for PeerKeyVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        rustls::server::ParsedCertificate::try_from(end_entity)?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not supported".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for PeerKeyVerifier {
    fn client_auth_mandatory(&self) -> bool {
        true
    }

    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        rustls::server::ParsedCertificate::try_from(end_entity)?;
        Ok(ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Err(rustls::Error::General("TLS 1.2 is not supported".into()))
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("proxishare-id-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn fingerprint_is_stable_across_launches() {
        let dir = temp_dir();
        let first = DeviceIdentity::load_or_create(&dir).unwrap();
        let second = DeviceIdentity::load_or_create(&dir).unwrap();
        assert_eq!(first.fingerprint, second.fingerprint);
        // The certificate is rebuilt each time, but only the key matters.
        assert_eq!(fingerprint_of(&second.cert_der).unwrap(), first.fingerprint);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn different_keys_have_different_fingerprints() {
        let a = DeviceIdentity::generate_ephemeral().unwrap();
        let b = DeviceIdentity::generate_ephemeral().unwrap();
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[cfg(unix)]
    #[test]
    fn key_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp_dir();
        DeviceIdentity::load_or_create(&dir).unwrap();
        let mode = std::fs::metadata(dir.join(IDENTITY_FILE)).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_key_file_is_an_error_not_a_new_identity() {
        let dir = temp_dir();
        std::fs::write(dir.join(IDENTITY_FILE), b"garbage").unwrap();
        assert!(DeviceIdentity::load_or_create(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
