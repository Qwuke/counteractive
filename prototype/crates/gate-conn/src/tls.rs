//! Inner TLS, client side (SPEC section 4).
//!
//! There is no CA: the server's certificate is self-signed and per boot,
//! and what makes it trustworthy is the attestation report that binds its
//! SPKI. So the verifier accepts any well-formed certificate, but still
//! checks the TLS 1.3 handshake signature, which proves the server holds
//! the private key for the SPKI the report is later checked against.

use std::sync::{Arc, OnceLock};

use attest::Role;
use rustls::client::Resumption;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, aws_lc_rs};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use rustls_pki_types::{CertificateDer, ServerName, UnixTime};
use sha2::{Digest as _, Sha256};

/// The only ALPN protocol either side offers.
pub(crate) const ALPN_HTTP1: &[u8] = b"http/1.1";

/// SHA-256 of a certificate's DER `SubjectPublicKeyInfo`: the value reports
/// bind. `None` if the certificate doesn't parse.
pub fn spki_sha256(certificate_der: &[u8]) -> Option<[u8; 32]> {
    let (_, certificate) = x509_parser::parse_x509_certificate(certificate_der).ok()?;
    Some(Sha256::digest(certificate.tbs_certificate.subject_pki.raw).into())
}

/// The fixed, meaningless host name for a role, used for the TLS server
/// name and the HTTP `Host` header. There is no real host to name.
pub(crate) fn host_name(role: Role) -> &'static str {
    match role {
        Role::Gate => "gate.invalid",
        Role::Storefront => "storefront.invalid",
    }
}

/// The TLS server name for a role. SNI is off, so it never leaves the
/// client; rustls just needs one.
pub fn server_name(role: Role) -> ServerName<'static> {
    ServerName::try_from(host_name(role)).expect("a valid DNS name")
}

/// The one crypto provider, so no other can be picked up by accident.
pub(crate) fn provider() -> Arc<CryptoProvider> {
    static PROVIDER: OnceLock<Arc<CryptoProvider>> = OnceLock::new();
    Arc::clone(PROVIDER.get_or_init(|| Arc::new(aws_lc_rs::default_provider())))
}

/// TLS 1.3 only, no resumption (so two conversations can't be linked by
/// tickets), no SNI, ALPN `http/1.1`, and [`AttestLater`] as the verifier.
pub(crate) fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    Arc::clone(CONFIG.get_or_init(|| {
        let provider = provider();
        let verifier = AttestLater { algorithms: provider.signature_verification_algorithms };
        let mut config = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .expect("aws-lc-rs supports TLS 1.3")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        config.resumption = Resumption::disabled();
        config.enable_sni = false;
        config.alpn_protocols = vec![ALPN_HTTP1.to_vec()];
        Arc::new(config)
    }))
}

/// Accepts any parseable certificate and leaves trust to attestation.
#[derive(Debug)]
struct AttestLater {
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for AttestLater {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        spki_sha256(end_entity).ok_or(rustls::Error::InvalidCertificate(rustls::CertificateError::BadEncoding))?;
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        // Only TLS 1.3 is offered, so this is never reached.
        Err(rustls::Error::PeerIncompatible(rustls::PeerIncompatible::Tls12NotOffered))
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
