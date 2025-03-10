//! # Ratchet tree extensions unit test
use super::*;

use crate::{messages::GroupSecrets, prelude::*, schedule::KeySchedule};
use mls_group::create_commit_params::CreateCommitParams;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::crypto::OpenMlsCrypto;
use tls_codec::Deserialize;

// This tests the ratchet tree extension to test if the duplicate detection works
ctest_ciphersuites!(duplicate_ratchet_tree_extension, test(ciphersuite_name: CiphersuiteName) {
    let crypto = OpenMlsRustCrypto::default();
    println!("Testing ciphersuite {:?}", ciphersuite_name);
    let ciphersuite = Config::ciphersuite(ciphersuite_name).unwrap();

    // Basic group setup.
    let group_aad = b"Alice's test group";

    // Define credential bundles
    let alice_credential_bundle = CredentialBundle::new(
        "Alice".into(),
        CredentialType::Basic,
        ciphersuite.signature_scheme(),
        &crypto,
    )
    .unwrap();
    let bob_credential_bundle = CredentialBundle::new(
        "Bob".into(),
        CredentialType::Basic,
        ciphersuite.signature_scheme(),
        &crypto,
    )
    .unwrap();

    // Generate KeyPackages
    let alice_key_package_bundle =
        KeyPackageBundle::new(&[ciphersuite.name()], &alice_credential_bundle, &crypto, Vec::new())
            .unwrap();

    let bob_key_package_bundle =
        KeyPackageBundle::new(&[ciphersuite.name()], &bob_credential_bundle, &crypto, Vec::new())
            .unwrap();
    let bob_key_package = bob_key_package_bundle.key_package();

    let config = MlsGroupConfig {
        add_ratchet_tree_extension: true,
        ..MlsGroupConfig::default()
    };

    let framing_parameters = FramingParameters::new(group_aad, WireFormat::MlsPlaintext);

    let mut alice_group = MlsGroup::builder(GroupId::random(&crypto), alice_key_package_bundle)
        .with_config(config)
        .build(&crypto)
        .expect("Error creating group.");

    // === Alice adds Bob ===
    let bob_add_proposal = alice_group
        .create_add_proposal(
            framing_parameters,
            &alice_credential_bundle,
            bob_key_package.clone(),
            &crypto,
        )
        .expect("Could not create proposal.");

    let proposal_store = ProposalStore::from_staged_proposal(StagedProposal::from_mls_plaintext(ciphersuite, &crypto, bob_add_proposal)
                            .expect("Could not create StagingProposal"));

    let params = CreateCommitParams::builder()
        .framing_parameters(framing_parameters)
        .credential_bundle(&alice_credential_bundle)
        .proposal_store(&proposal_store)
        .force_self_update(false)
        .build();
    let (mls_plaintext_commit, welcome_bundle_alice_bob_option, _kpb_option) = alice_group
        .create_commit(
            params,
            &crypto,
        )
        .expect("Error creating commit");

    let staged_commit = alice_group
        .stage_commit(&mls_plaintext_commit, &proposal_store, &[], None, &crypto)
        .expect("error staging commit");
    alice_group.merge_commit(staged_commit);

    let mut welcome = welcome_bundle_alice_bob_option.expect("Expected a Welcome message");

    //  === Duplicate the ratchet tree extension ===

    // Find key_package in welcome secrets
    let egs = MlsGroup::find_key_package_from_welcome_secrets(
        bob_key_package_bundle.key_package(),
        welcome.secrets(),
        &crypto,
    ).expect("JoinerSecret not found");

    let group_secrets_bytes = crypto
        .crypto()
        .hpke_open(
            ciphersuite.hpke_config(),
            &egs.encrypted_group_secrets,
            bob_key_package_bundle.private_key().as_slice(),
            &[],
            &[],
        ).expect("Could not decrypt group secrets");
    let group_secrets = GroupSecrets::tls_deserialize(&mut group_secrets_bytes.as_slice()).expect("Could not decode GroupSecrets").config(ciphersuite, ProtocolVersion::default());
    let joiner_secret = group_secrets.joiner_secret;

    // Create key schedule
    let key_schedule = KeySchedule::init(
        ciphersuite,
        &crypto,
        joiner_secret,
        psk_output(ciphersuite, &crypto, None, &group_secrets.psks).expect("Could not extract PSKs"),
    );

    // Derive welcome key & noce from the key schedule
    let (welcome_key, welcome_nonce) = key_schedule
        .welcome(&crypto).expect("Expected a WelcomeSecret")
        .derive_welcome_key_nonce(&crypto);

    let group_info_bytes = welcome_key
        .aead_open(&crypto, welcome.encrypted_group_info(), &[], &welcome_nonce)
        .map_err(|_| WelcomeError::GroupInfoDecryptionFailure).expect("Could not decrypt GroupInfo");
    let mut group_info = GroupInfo::tls_deserialize(&mut group_info_bytes.as_slice()).expect("Could not decode GroupInfo");

    // Duplicate extensions
    let extensions = group_info.other_extensions();
    let duplicate_extensions = vec![extensions[0].clone(), extensions[0].clone()];
    group_info.set_other_extensions(duplicate_extensions);

    // Put everything back together
    let group_info = group_info.re_sign(&bob_credential_bundle, &crypto).expect("Error re-signing GroupInfo");

    let encrypted_group_info = welcome_key
        .aead_seal(&crypto, &group_info.tls_serialize_detached().expect("Could not encode GroupInfo"), &[], &welcome_nonce)
        .unwrap();

    welcome.set_encrypted_group_info(encrypted_group_info);

    // Try to join group
    let error = MlsGroup::new_from_welcome(
        welcome,
        None,
        bob_key_package_bundle,
        None,
        &crypto,
    )
    .err();

    // We expect an error because the ratchet tree is duplicated
    assert_eq!(
        error.expect("We expected an error"),
        MlsGroupError::WelcomeError(WelcomeError::DuplicateRatchetTreeExtension)
    );
});
