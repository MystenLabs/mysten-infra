// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{crypto::CertVerifier, Result};
use quinn::IdleTimeout;
use rccheck::{ed25519_certgen::Ed25519, rustls, Certifiable};
use serde::{Deserialize, Serialize};
use std::{future::Future, sync::Arc, time::Duration};

#[derive(Debug, Default)]
pub struct EndpointConfigBuilder {
    pub keypair: Option<ed25519_dalek::Keypair>,

    // TODO Maybe use server name to identify the network name?
    //
    /// Note that the end-entity certificate must have the
    /// [Subject Alternative Name](https://tools.ietf.org/html/rfc6125#section-4.1)
    /// extension to describe, e.g., the valid DNS name.
    pub server_name: Option<String>,

    /// How long to wait to hear from a peer before timing out a connection.
    ///
    /// In the absence of any keep-alive messages, connections will be closed if they remain idle
    /// for at least this duration.
    ///
    /// If unspecified, this will default to [`EndpointConfig::DEFAULT_IDLE_TIMEOUT`].
    ///
    /// Maximum possible value is 2^62.
    pub idle_timeout: Option<Duration>,

    /// Interval at which to send keep-alives to maintain otherwise idle connections.
    ///
    /// Keep-alives prevent otherwise idle connections from timing out.
    ///
    /// If unspecified, this will default to `None`, disabling keep-alives.
    pub keep_alive_interval: Option<Duration>,

    /// Retry configurations for establishing connections and sending messages.
    /// Determines the retry behaviour of requests, by setting the back off strategy used.
    pub retry_config: Option<RetryConfig>,
}

impl EndpointConfigBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn build(self) -> Result<EndpointConfig> {
        let keypair = self.keypair.unwrap();
        let server_name = self.server_name.unwrap();
        let idle_timeout = self
            .idle_timeout
            .unwrap_or(EndpointConfig::DEFAULT_IDLE_TIMEOUT);
        let keep_alive_interval = self.keep_alive_interval;
        let retry_config = self.retry_config;

        let cert_verifier = Arc::new(CertVerifier(server_name.clone()));
        let (certificate, pkcs8_der) = Self::generate_cert(&keypair, &server_name);

        let transport_config = Self::transport_config(idle_timeout, keep_alive_interval)?;
        let server_config = Self::server_config(
            certificate.clone(),
            pkcs8_der.clone(),
            cert_verifier.clone(),
            transport_config.clone(),
        )?;
        let client_config = Self::client_config(
            certificate.clone(),
            pkcs8_der.clone(),
            cert_verifier,
            transport_config,
        )?;

        Ok(EndpointConfig {
            _certificate: certificate,
            _pkcs8_der: pkcs8_der,
            keypair,
            quinn_server_config: server_config,
            quinn_client_config: client_config,
            server_name,
            _retry_config: retry_config,
        })
    }

    fn generate_cert(
        keypair: &ed25519_dalek::Keypair,
        server_name: &str,
    ) -> (rustls::Certificate, rustls::PrivateKey) {
        let key_der = rustls::PrivateKey(keypair.to_pkcs8_bytes());
        let keypair = ed25519_dalek::Keypair::from_bytes(&keypair.to_bytes()).unwrap();
        let certificate =
            Ed25519::keypair_to_certificate(vec![server_name.to_owned()], keypair).unwrap();
        (certificate, key_der)
    }

    fn transport_config(
        idle_timeout: Duration,
        keep_alive_interval: Option<Duration>,
    ) -> Result<Arc<quinn::TransportConfig>> {
        let idle_timeout = IdleTimeout::try_from(idle_timeout)?;
        let mut config = quinn::TransportConfig::default();

        config
            .max_idle_timeout(Some(idle_timeout))
            .keep_alive_interval(keep_alive_interval);

        Ok(Arc::new(config))
    }

    fn server_config(
        cert: rustls::Certificate,
        pkcs8_der: rustls::PrivateKey,
        cert_verifier: Arc<CertVerifier>,
        transport_config: Arc<quinn::TransportConfig>,
    ) -> Result<quinn::ServerConfig> {
        let server_crypto = rustls::ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(cert_verifier)
            .with_single_cert(vec![cert], pkcs8_der)?;

        let mut server = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        server.transport = transport_config;
        Ok(server)
    }

    fn client_config(
        cert: rustls::Certificate,
        pkcs8_der: rustls::PrivateKey,
        cert_verifier: Arc<CertVerifier>,
        transport_config: Arc<quinn::TransportConfig>,
    ) -> Result<quinn::ClientConfig> {
        // setup certificates
        let mut roots = rustls::RootCertStore::empty();
        roots.add(&cert).map_err(|_e| ConfigError::Webpki)?;

        let client_crypto = rustls::ClientConfig::builder()
            .with_safe_defaults()
            .with_custom_certificate_verifier(cert_verifier)
            .with_single_cert(vec![cert], pkcs8_der)?;

        let mut client = quinn::ClientConfig::new(Arc::new(client_crypto));
        client.transport = transport_config;
        Ok(client)
    }
}

#[derive(Debug)]
pub struct EndpointConfig {
    _certificate: rustls::Certificate,
    _pkcs8_der: rustls::PrivateKey,
    keypair: ed25519_dalek::Keypair,
    quinn_server_config: quinn::ServerConfig,
    quinn_client_config: quinn::ClientConfig,

    // TODO Maybe use server name to identify the network name?
    //
    /// Note that the end-entity certificate must have the
    /// [Subject Alternative Name](https://tools.ietf.org/html/rfc6125#section-4.1)
    /// extension to describe, e.g., the valid DNS name.
    server_name: String,

    /// Retry configurations for establishing connections and sending messages.
    /// Determines the retry behaviour of requests, by setting the back off strategy used.
    _retry_config: Option<RetryConfig>,
}

impl EndpointConfig {
    /// Default for [`EndpointConfig::idle_timeout`] (1 minute).
    ///
    /// This is based on average time in which routers would close the UDP mapping to the peer if they
    /// see no conversation between them.
    pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

    pub fn builder() -> EndpointConfigBuilder {
        EndpointConfigBuilder::new()
    }

    pub fn keypair(&self) -> &ed25519_dalek::Keypair {
        &self.keypair
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    pub fn server_config(&self) -> &quinn::ServerConfig {
        &self.quinn_server_config
    }

    pub fn client_config(&self) -> &quinn::ClientConfig {
        &self.quinn_client_config
    }

    #[cfg(test)]
    pub(crate) fn random(server_name: &str) -> Self {
        let mut rng = rand::thread_rng();
        let keypair = ed25519_dalek::Keypair::generate(&mut rng);

        let mut builder = Self::builder();
        builder.keypair = Some(keypair);
        builder.server_name = Some(server_name.to_owned());

        builder.build().unwrap()
    }
}

/// This type provides serialized bytes for a private key.
///
/// The private key must be DER-encoded ASN.1 in either
/// PKCS#8 or PKCS#1 format.
// TODO: move this to rccheck?
trait ToPKCS8 {
    fn to_pkcs8_bytes(&self) -> Vec<u8>;
}

impl ToPKCS8 for ed25519_dalek::Keypair {
    fn to_pkcs8_bytes(&self) -> Vec<u8> {
        use ed25519::pkcs8::EncodePrivateKey;

        let kpb = ed25519::KeypairBytes {
            secret_key: self.secret.to_bytes(),
            public_key: None,
        };
        let pkcs8 = kpb.to_pkcs8_der().unwrap();
        pkcs8.as_ref().to_vec()
    }
}

/// Retry configurations for establishing connections and sending messages.
/// Determines the retry behaviour of requests, by setting the back off strategy used.
#[derive(Clone, Debug, Copy, Serialize, Deserialize)]
pub struct RetryConfig {
    /// The initial retry interval.
    ///
    /// This is the first delay before a retry, for establishing connections and sending messages.
    /// The subsequent delay will be decided by the `retry_delay_multiplier`.
    pub initial_retry_interval: Duration,

    /// The maximum value of the back off period. Once the retry interval reaches this
    /// value it stops increasing.
    ///
    /// This is the longest duration we will have,
    /// for establishing connections and sending messages.
    /// Retrying continues even after the duration times have reached this duration.
    /// The number of retries before that happens, will be decided by the `retry_delay_multiplier`.
    /// The number of retries after that, will be decided by the `retrying_max_elapsed_time`.
    pub max_retry_interval: Duration,

    /// The value to multiply the current interval with for each retry attempt.
    pub retry_delay_multiplier: f64,

    /// The randomization factor to use for creating a range around the retry interval.
    ///
    /// A randomization factor of 0.5 results in a random period ranging between 50% below and 50%
    /// above the retry interval.
    pub retry_delay_rand_factor: f64,

    /// The maximum elapsed time after instantiating
    ///
    /// Retrying continues until this time has elapsed.
    /// The number of retries before that happens, will be decided by the other retry config options.
    pub retrying_max_elapsed_time: Duration,
}

impl RetryConfig {
    // Together with the default max and multiplier,
    // default gives 5-6 retries in ~30 s total retry time.

    /// Default for [`RetryConfig::max_retry_interval`] (500 ms).
    pub const DEFAULT_INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(500);

    /// Default for [`RetryConfig::max_retry_interval`] (15 s).
    pub const DEFAULT_MAX_RETRY_INTERVAL: Duration = Duration::from_secs(15);

    /// Default for [`RetryConfig::retry_delay_multiplier`] (x1.5).
    pub const DEFAULT_RETRY_INTERVAL_MULTIPLIER: f64 = 1.5;

    /// Default for [`RetryConfig::retry_delay_rand_factor`] (0.3).
    pub const DEFAULT_RETRY_DELAY_RAND_FACTOR: f64 = 0.3;

    /// Default for [`RetryConfig::retrying_max_elapsed_time`] (30 s).
    pub const DEFAULT_RETRYING_MAX_ELAPSED_TIME: Duration = Duration::from_secs(30);

    // Perform `op` and retry on errors as specified by this configuration.
    //
    // Note that `backoff::Error<E>` implements `From<E>` for any `E` by creating a
    // `backoff::Error::Transient`, meaning that errors will be retried unless explicitly returning
    // `backoff::Error::Permanent`.
    pub fn retry<R, E, Fn, Fut>(&self, op: Fn) -> impl Future<Output = Result<R, E>>
    where
        Fn: FnMut() -> Fut,
        Fut: Future<Output = Result<R, backoff::Error<E>>>,
    {
        let backoff = backoff::ExponentialBackoff {
            initial_interval: self.initial_retry_interval,
            randomization_factor: self.retry_delay_rand_factor,
            multiplier: self.retry_delay_multiplier,
            max_interval: self.max_retry_interval,
            max_elapsed_time: Some(self.retrying_max_elapsed_time),
            ..Default::default()
        };
        backoff::future::retry(backoff, op)
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            initial_retry_interval: RetryConfig::DEFAULT_INITIAL_RETRY_INTERVAL,
            max_retry_interval: RetryConfig::DEFAULT_MAX_RETRY_INTERVAL,
            retry_delay_multiplier: RetryConfig::DEFAULT_RETRY_INTERVAL_MULTIPLIER,
            retry_delay_rand_factor: RetryConfig::DEFAULT_RETRY_DELAY_RAND_FACTOR,
            retrying_max_elapsed_time: RetryConfig::DEFAULT_RETRYING_MAX_ELAPSED_TIME,
        }
    }
}

/// An error that occured when generating the TLS certificate.
#[derive(Debug, thiserror::Error)]
#[error(transparent)]
struct CertificateGenerationError(
    // Though there are multiple different errors that could occur by the code, since we are
    // generating a certificate, they should only really occur due to buggy implementations. As
    // such, we don't attempt to expose more detail than 'something went wrong', which will
    // hopefully be enough for someone to file a bug report...
    Box<dyn std::error::Error + Send + Sync>,
);

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
enum ConfigError {
    #[error("An error occurred when generating the TLS certificate")]
    CertificateGeneration(#[from] CertificateGenerationError),

    #[error("An error occurred parsing idle timeout duration")]
    InvalidIdleTimeout(#[from] quinn_proto::VarIntBoundsExceeded),

    #[error("An error occurred within rustls")]
    Rustls(#[from] rustls::Error),

    #[error("An error occurred generating client config certificates")]
    Webpki,
}
