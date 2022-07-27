// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{ed25519_certgen::Ed25519, *};
use rcgen::generate_simple_self_signed;
use rustls::{client::ServerCertVerifier, server::ClientCertVerifier};
use x509_parser::traits::FromDer;

fn cert_bytes_to_spki_bytes(cert_bytes: &[u8]) -> Vec<u8> {
    let cert_parsed = X509Certificate::from_der(cert_bytes)
        .map_err(|_| rustls::Error::InvalidCertificateEncoding)
        .unwrap();
    let spki = cert_parsed.1.public_key().clone();
    spki.raw.to_vec()
}

#[test]
fn serde_round_trip() {
    let subject_alt_names = vec!["localhost".to_string()];

    let cert = generate_simple_self_signed(subject_alt_names).unwrap();
    let cert_bytes: Vec<u8> = cert.serialize_der().unwrap();

    let spki = cert_bytes_to_spki_bytes(&cert_bytes);
    let psk = Psk::from_der(&spki).unwrap();
    let psk_bytes = bincode::serialize(&psk).unwrap();
    let psk_roundtripped = bincode::deserialize::<Psk>(&psk_bytes).unwrap();
    assert_eq!(psk, psk_roundtripped);
}

#[test]
fn rc_gen_self_signed_dalek() {
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_seed([0; 32]);
    let kp = ed25519_dalek::Keypair::generate(&mut rng);
    let kp2 = ed25519_dalek::Keypair::generate(&mut rng);

    let subject_alt_names = vec!["localhost".to_string()];

    let public_key = kp.public;
    let public_key_2 = kp2.public;

    let spki = Ed25519::public_key_to_spki(&public_key);
    let spki2 = Ed25519::public_key_to_spki(&public_key_2);

    let psk_set = PskSet::from_der(&[&spki[..], &spki2[..]]).unwrap();
    let now = SystemTime::now();

    let cert = Ed25519::keypair_to_certificate(subject_alt_names, kp).unwrap();

    // this passes client verification
    psk_set.verify_client_cert(&cert, &[], now).unwrap();

    // this passes server verification
    let mut empty = std::iter::empty();
    psk_set
        .verify_server_cert(
            &cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now,
        )
        .unwrap();
}

#[test]
fn rc_gen_not_self_signed_dalek() {
    let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::from_seed([0; 32]);
    let kp = ed25519_dalek::Keypair::generate(&mut rng);
    let invalid_kp = ed25519_dalek::Keypair::generate(&mut rng);

    let subject_alt_names = vec!["localhost".to_string()];

    let public_key = kp.public;

    let spki = Ed25519::public_key_to_spki(&public_key);

    let psk_set = PskSet::from_der(&[&spki[..]]).unwrap();
    let now = SystemTime::now();

    let invalid_cert = Ed25519::keypair_to_certificate(subject_alt_names, invalid_kp).unwrap();

    // this does not pass client verification
    assert!(psk_set
        .verify_client_cert(&invalid_cert, &[], now)
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));

    // this passes server verification
    let mut empty = std::iter::empty();
    assert!(psk_set
        .verify_server_cert(
            &invalid_cert,
            &[],
            &rustls::ServerName::try_from("localhost").unwrap(),
            &mut empty,
            &[],
            now,
        )
        .err()
        .unwrap()
        .to_string()
        .contains("invalid peer certificate"));
}
