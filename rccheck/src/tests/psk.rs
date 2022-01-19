// Copyright(C) 2022, Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::*;
use rcgen::generate_simple_self_signed;
use rustls::{client::ServerCertVerifier, server::ClientCertVerifier};
use x509_parser::traits::FromDer;

#[test]
fn rc_gen_self_client() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();
    let cert_parsed = X509Certificate::from_der(&cert_bytes[..])
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)
        .unwrap();
    let spki = cert_parsed.1.public_key().clone();
    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes.clone());

    assert!(Psk(spki).verify_client_cert(&rstls_cert, &[], now).is_ok());
}

#[test]
fn rc_gen_not_self_client() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names.clone()).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();

    let other_cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let other_bytes: Vec<u8> = other_cert.serialize_der().unwrap();
    let other_cert_parsed = X509Certificate::from_der(&other_bytes[..])
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)
        .unwrap();
    let spki = other_cert_parsed.1.public_key().clone();
    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes);

    assert!(Psk(spki)
        .verify_client_cert(&rstls_cert, &[], now)
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));
}

#[test]
fn rc_gen_self_server() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();
    let cert_parsed = X509Certificate::from_der(&cert_bytes[..])
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)
        .unwrap();
    let spki = cert_parsed.1.public_key().clone();
    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes.clone());

    let mut empty = std::iter::empty();

    assert!(Psk(spki)
        .verify_server_cert(
            &rstls_cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now
        )
        .is_ok());
}

#[test]
fn rc_gen_not_self_server() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names.clone()).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();

    let other_cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let other_bytes: Vec<u8> = other_cert.serialize_der().unwrap();
    let other_cert_parsed = X509Certificate::from_der(&other_bytes[..])
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)
        .unwrap();
    let spki = other_cert_parsed.1.public_key().clone();
    let now = SystemTime::now();
    let rstls_cert = rustls::Certificate(cert_bytes);

    let mut empty = std::iter::empty();

    assert!(Psk(spki)
        .verify_server_cert(
            &rstls_cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now
        )
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));
}
