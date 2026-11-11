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

/// Real QUIC handshakes over loopback, using the production TLS configs. These
/// cover what every connection relies on: that `peer_fingerprint` names a key
/// the peer actually holds, and that the trust store rejects any other key.
#[cfg(test)]
mod handshake_tests {
    use super::*;
    use crate::crypto::security::{PeerTrust, SecurityService, TrustedDevice};
    use rustls::sign::CertifiedKey;
    use std::time::Duration;

    const STEP_TIMEOUT: Duration = Duration::from_secs(5);

    /// What each side saw: the peer's fingerprint, or why the handshake failed.
    struct Outcome {
        client: Result<String, String>,
        server: Result<String, String>,
    }

    async fn handshake(server_config: rustls::ServerConfig, client_config: rustls::ClientConfig) -> Outcome {
        let server_crypto = quinn::crypto::rustls::QuicServerConfig::try_from(server_config).unwrap();
        let server = quinn::Endpoint::server(
            quinn::ServerConfig::with_crypto(Arc::new(server_crypto)),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let client_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(client_config).unwrap();
        let client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        let connecting = client
            .connect_with(
                quinn::ClientConfig::new(Arc::new(client_crypto)),
                server.local_addr().unwrap(),
                "proxishare.local",
            )
            .unwrap();

        // Each side is bounded on its own, so a failure on one side can't hang the test.
        let client_side = async {
            let connection = tokio::time::timeout(STEP_TIMEOUT, connecting)
                .await
                .map_err(|_| "timed out".to_string())?
                .map_err(|e| e.to_string())?;
            peer_fingerprint(&connection).map_err(|e| e.to_string())
        };
        let server_side = async {
            let connection = tokio::time::timeout(STEP_TIMEOUT, async {
                server.accept().await.ok_or("endpoint closed")?.await.map_err(|e| e.to_string())
            })
            .await
            .map_err(|_| "timed out".to_string())??;
            peer_fingerprint(&connection).map_err(|e| e.to_string())
        };
        let (client, server) = tokio::join!(client_side, server_side);
        Outcome { client, server }
    }

    /// Presents `cert_of`'s certificate while signing with `key_of`'s key: what
    /// an attacker can do after copying a paired device's (public) certificate.
    fn forged(cert_of: &DeviceIdentity, key_of: &DeviceIdentity) -> Arc<CertifiedKey> {
        let key = provider().key_provider.load_private_key(key_of.private_key()).unwrap();
        Arc::new(CertifiedKey::new(cert_of.cert_chain(), key))
    }

    #[derive(Debug)]
    struct Present(Option<Arc<CertifiedKey>>);

    impl rustls::server::ResolvesServerCert for Present {
        fn resolve(&self, _: rustls::server::ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
            self.0.clone()
        }
    }

    impl rustls::client::ResolvesClientCert for Present {
        fn resolve(&self, _: &[&[u8]], _: &[SignatureScheme]) -> Option<Arc<CertifiedKey>> {
            self.0.clone()
        }

        fn has_certs(&self) -> bool {
            self.0.is_some()
        }
    }

    fn server_presenting(identity: &DeviceIdentity, cert: Arc<CertifiedKey>) -> rustls::ServerConfig {
        let mut config = identity.get_server_config().unwrap();
        config.cert_resolver = Arc::new(Present(Some(cert)));
        config
    }

    fn client_presenting(identity: &DeviceIdentity, cert: Option<Arc<CertifiedKey>>) -> rustls::ClientConfig {
        let mut config = identity.get_client_config().unwrap();
        config.client_auth_cert_resolver = Arc::new(Present(cert));
        config
    }

    /// A trust store with `device_id` paired to `identity`'s key.
    fn paired(device_id: &str, identity: &DeviceIdentity) -> SecurityService {
        let dir = std::env::temp_dir().join(format!("proxishare-hs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut security = SecurityService::load(dir, "me".into()).unwrap();
        security
            .add_trusted(TrustedDevice {
                id: device_id.into(),
                name: "Laptop".into(),
                last_ip: "127.0.0.1".into(),
                last_port: 51731,
                last_seen: 0,
                fingerprint: Some(identity.fingerprint.clone()),
            })
            .unwrap();
        security
    }

    #[tokio::test]
    async fn each_side_learns_the_other_sides_key() {
        let server = DeviceIdentity::generate_ephemeral().unwrap();
        let client = DeviceIdentity::generate_ephemeral().unwrap();
        let outcome = handshake(server.get_server_config().unwrap(), client.get_client_config().unwrap()).await;
        assert_eq!(outcome.client.unwrap(), server.fingerprint);
        assert_eq!(outcome.server.unwrap(), client.fingerprint);
    }

    #[tokio::test]
    async fn sending_to_an_impostor_is_a_key_mismatch() {
        let laptop = DeviceIdentity::generate_ephemeral().unwrap();
        let impostor = DeviceIdentity::generate_ephemeral().unwrap();
        let me = DeviceIdentity::generate_ephemeral().unwrap();
        let security = paired("laptop", &laptop);

        // What `connect_verified` checks before sending anything.
        let real = handshake(laptop.get_server_config().unwrap(), me.get_client_config().unwrap()).await;
        assert_eq!(security.verify_peer("laptop", &real.client.unwrap()), PeerTrust::Verified);

        let fake = handshake(impostor.get_server_config().unwrap(), me.get_client_config().unwrap()).await;
        assert_eq!(security.verify_peer("laptop", &fake.client.unwrap()), PeerTrust::KeyMismatch);
    }

    #[tokio::test]
    async fn receiving_from_an_impostor_is_a_key_mismatch() {
        let laptop = DeviceIdentity::generate_ephemeral().unwrap();
        let impostor = DeviceIdentity::generate_ephemeral().unwrap();
        let me = DeviceIdentity::generate_ephemeral().unwrap();
        let security = paired("laptop", &laptop);

        // What `FileReceiver` checks before accepting an offer or history.
        let real = handshake(me.get_server_config().unwrap(), laptop.get_client_config().unwrap()).await;
        assert_eq!(security.verify_peer("laptop", &real.server.unwrap()), PeerTrust::Verified);

        let fake = handshake(me.get_server_config().unwrap(), impostor.get_client_config().unwrap()).await;
        assert_eq!(security.verify_peer("laptop", &fake.server.unwrap()), PeerTrust::KeyMismatch);
    }

    #[tokio::test]
    async fn server_with_a_copied_certificate_is_rejected() {
        let laptop = DeviceIdentity::generate_ephemeral().unwrap();
        let impostor = DeviceIdentity::generate_ephemeral().unwrap();
        let me = DeviceIdentity::generate_ephemeral().unwrap();

        let outcome = handshake(
            server_presenting(&impostor, forged(&laptop, &impostor)),
            me.get_client_config().unwrap(),
        )
        .await;
        assert!(outcome.client.is_err(), "client accepted a forged server: {:?}", outcome.client);
    }

    #[tokio::test]
    async fn client_with_a_copied_certificate_is_rejected() {
        let laptop = DeviceIdentity::generate_ephemeral().unwrap();
        let impostor = DeviceIdentity::generate_ephemeral().unwrap();
        let me = DeviceIdentity::generate_ephemeral().unwrap();

        let outcome = handshake(
            me.get_server_config().unwrap(),
            client_presenting(&impostor, Some(forged(&laptop, &impostor))),
        )
        .await;
        assert!(outcome.server.is_err(), "server accepted a forged client: {:?}", outcome.server);
    }

    #[tokio::test]
    async fn client_without_a_certificate_is_rejected() {
        let me = DeviceIdentity::generate_ephemeral().unwrap();
        let anonymous = DeviceIdentity::generate_ephemeral().unwrap();

        // Refused by the handshake (client auth is mandatory) and, failing
        // that, by `peer_fingerprint`.
        let outcome = handshake(me.get_server_config().unwrap(), client_presenting(&anonymous, None)).await;
        assert!(outcome.server.is_err(), "server accepted a client with no certificate: {:?}", outcome.server);
    }
}
