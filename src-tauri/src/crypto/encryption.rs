use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use std::sync::Arc;

pub struct CertificateManager {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

impl CertificateManager {
    pub fn generate_self_signed() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let cert_params = rcgen::CertificateParams::new(vec!["proxishare.local".to_string()]);
        let cert = rcgen::Certificate::from_params(cert_params)?;

        Ok(Self {
            cert_der: cert.serialize_der()?,
            key_der: cert.serialize_private_key_der(),
        })
    }

    pub fn get_server_config(
        &self,
    ) -> Result<rustls::ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
        let cert_der = CertificateDer::from(self.cert_der.clone());
        let key_der = PrivateKeyDer::try_from(self.key_der.clone())?;

        let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)?;

        config.alpn_protocols = vec![b"proxishare".to_vec()];
        Ok(config)
    }

    pub fn get_client_config(
        &self,
    ) -> Result<rustls::ClientConfig, Box<dyn std::error::Error + Send + Sync>> {
        let mut config = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();

        config.alpn_protocols = vec![b"proxishare".to_vec()];
        Ok(config)
    }
}

#[derive(Debug)]
struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}
