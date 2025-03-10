//! # Known Answer Tests for the transcript hashes
//!
//! See https://github.com/mlswg/mls-implementations/blob/master/test-vectors.md
//! for more description on the test vectors.

use std::convert::TryFrom;

#[cfg(test)]
use crate::test_utils::{read, write};

use crate::{
    ciphersuite::{signable::Verifiable, Ciphersuite, CiphersuiteName, Secret},
    config::{Config, ProtocolVersion},
    credentials::{Credential, CredentialBundle, CredentialType},
    group::{
        update_confirmed_transcript_hash, update_interim_transcript_hash, GroupContext, GroupEpoch,
        GroupId, WireFormat,
    },
    messages::Commit,
    prelude::{
        random_u32, random_u64, FramingParameters, LeafIndex, MlsPlaintext,
        MlsPlaintextCommitAuthData, MlsPlaintextCommitContent, VerifiableMlsPlaintext,
    },
    schedule::{ConfirmationKey, MembershipKey},
    test_utils::{bytes_to_hex, hex_to_bytes},
};

use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::{random::OpenMlsRand, types::SignatureScheme, OpenMlsCryptoProvider};
use serde::{self, Deserialize, Serialize};
use tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerializeTrait};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TranscriptTestVector {
    pub cipher_suite: u16,
    group_id: String,
    epoch: u64,
    tree_hash_before: String,
    confirmed_transcript_hash_before: String,
    interim_transcript_hash_before: String,
    credential: String,
    membership_key: String,
    confirmation_key: String,
    commit: String, // TLS serialized MlsPlaintext(Commit)

    group_context: String,
    confirmed_transcript_hash_after: String,
    interim_transcript_hash_after: String,
}

pub fn generate_test_vector(ciphersuite: &'static Ciphersuite) -> TranscriptTestVector {
    let crypto = OpenMlsRustCrypto::default();
    // Generate random values.
    let group_id = GroupId::random(&crypto);
    let epoch = random_u64();
    let tree_hash_before = crypto.rand().random_vec(ciphersuite.hash_length()).unwrap();
    let confirmed_transcript_hash_before =
        crypto.rand().random_vec(ciphersuite.hash_length()).unwrap();
    let interim_transcript_hash_before =
        crypto.rand().random_vec(ciphersuite.hash_length()).unwrap();
    let membership_key = MembershipKey::from_secret(Secret::random(
        ciphersuite,
        &crypto,
        None, /* MLS version */
    ));
    let confirmation_key = ConfirmationKey::from_secret(Secret::random(
        ciphersuite,
        &crypto,
        None, /* MLS version */
    ));

    // Build plaintext commit message.
    let credential_bundle = CredentialBundle::new(
        b"client".to_vec(),
        CredentialType::Basic,
        SignatureScheme::from(ciphersuite.name()),
        &crypto,
    )
    .unwrap();
    let context = GroupContext::new(
        group_id.clone(),
        GroupEpoch(epoch),
        tree_hash_before.clone(),
        confirmed_transcript_hash_before.clone(),
        &[], // extensions
    )
    .expect("Error creating group context");
    let aad = crypto.rand().random_vec(48).unwrap();
    let framing_parameters = FramingParameters::new(&aad, WireFormat::MlsPlaintext);
    let mut commit = MlsPlaintext::new_commit(
        framing_parameters,
        LeafIndex::from(random_u32()),
        Commit {
            proposals: vec![].into(),
            path: None,
        },
        &credential_bundle,
        &context,
        &crypto,
    )
    .unwrap();

    let confirmed_transcript_hash_after = update_confirmed_transcript_hash(
        ciphersuite,
        &crypto,
        &MlsPlaintextCommitContent::try_from(&commit).unwrap(),
        &interim_transcript_hash_before,
    )
    .expect("Error updating confirmed transcript hash");
    let confirmation_tag = confirmation_key.tag(&crypto, &confirmed_transcript_hash_after);
    commit.set_confirmation_tag(confirmation_tag);

    let interim_transcript_hash_after = update_interim_transcript_hash(
        ciphersuite,
        &crypto,
        &MlsPlaintextCommitAuthData::try_from(&commit).unwrap(),
        &confirmed_transcript_hash_after,
    )
    .expect("Error updating interim transcript hash");
    commit
        .set_membership_tag(
            &crypto,
            &context.tls_serialize_detached().unwrap(),
            &membership_key,
        )
        .expect("Error adding membership tag");
    let credential = credential_bundle
        .credential()
        .tls_serialize_detached()
        .unwrap();

    TranscriptTestVector {
        cipher_suite: ciphersuite.name() as u16,
        group_id: bytes_to_hex(group_id.as_slice()),
        epoch,
        tree_hash_before: bytes_to_hex(&tree_hash_before),
        confirmed_transcript_hash_before: bytes_to_hex(&confirmed_transcript_hash_before),
        interim_transcript_hash_before: bytes_to_hex(&interim_transcript_hash_before),
        credential: bytes_to_hex(&credential),
        membership_key: bytes_to_hex(membership_key.as_slice()),
        confirmation_key: bytes_to_hex(confirmation_key.as_slice()),
        commit: bytes_to_hex(
            &commit
                .tls_serialize_detached()
                .expect("Error encoding commit"),
        ),

        group_context: bytes_to_hex(&context.tls_serialize_detached().unwrap()),
        confirmed_transcript_hash_after: bytes_to_hex(&confirmed_transcript_hash_after),
        interim_transcript_hash_after: bytes_to_hex(&interim_transcript_hash_after),
    }
}

#[test]
fn write_test_vectors() {
    let mut tests = Vec::new();
    const NUM_TESTS: usize = 100;

    for ciphersuite in Config::supported_ciphersuites() {
        for _ in 0..NUM_TESTS {
            let test = generate_test_vector(ciphersuite);
            tests.push(test);
        }
    }

    write("test_vectors/kat_transcripts-new.json", &tests);
}

pub fn run_test_vector(test_vector: TranscriptTestVector) -> Result<(), TranscriptTestVectorError> {
    let ciphersuite =
        CiphersuiteName::try_from(test_vector.cipher_suite).expect("Invalid ciphersuite");
    let crypto = OpenMlsRustCrypto::default();
    let ciphersuite = match Config::ciphersuite(ciphersuite) {
        Ok(cs) => cs,
        Err(_) => {
            log::info!(
                "Unsupported ciphersuite {} in test vector. Skipping ...",
                ciphersuite
            );
            return Ok(());
        }
    };
    log::debug!("Testing test vector for ciphersuite {:?}", ciphersuite);
    log::trace!("  {:?}", test_vector);

    // Read input values.
    let group_id = GroupId {
        value: hex_to_bytes(&test_vector.group_id).into(),
    };
    let epoch = test_vector.epoch;
    let tree_hash_before = hex_to_bytes(&test_vector.tree_hash_before);
    let confirmed_transcript_hash_before =
        hex_to_bytes(&test_vector.confirmed_transcript_hash_before);
    let interim_transcript_hash_before = hex_to_bytes(&test_vector.interim_transcript_hash_before);
    let membership_key = MembershipKey::from_secret(Secret::from_slice(
        &hex_to_bytes(&test_vector.membership_key),
        ProtocolVersion::default(),
        ciphersuite,
    ));
    let confirmation_key = ConfirmationKey::from_secret(Secret::from_slice(
        &hex_to_bytes(&test_vector.confirmation_key),
        ProtocolVersion::default(),
        ciphersuite,
    ));
    let credential =
        Credential::tls_deserialize(&mut hex_to_bytes(&test_vector.credential).as_slice()).unwrap();

    // Check membership and confirmation tags.
    let commit_bytes = hex_to_bytes(&test_vector.commit);
    let mut commit = VerifiableMlsPlaintext::tls_deserialize(&mut commit_bytes.as_slice())
        .expect("Error decoding commit");
    let context = GroupContext::new(
        group_id,
        GroupEpoch(epoch),
        tree_hash_before,
        confirmed_transcript_hash_before,
        &[], // extensions
    )
    .expect("Error creating group context");
    let expected_group_context = hex_to_bytes(&test_vector.group_context);
    if context.tls_serialize_detached().unwrap() != expected_group_context {
        log::error!("  Group context mismatch");
        log::debug!(
            "    Computed: {:x?}",
            context.tls_serialize_detached().unwrap()
        );
        log::debug!("    Expected: {:x?}", expected_group_context);
        if cfg!(test) {
            panic!("Group context mismatch");
        }
        return Err(TranscriptTestVectorError::GroupContextMismatch);
    }
    commit.set_context(context.tls_serialize_detached().unwrap());
    if commit.verify_membership(&crypto, &membership_key).is_err() {
        if cfg!(test) {
            panic!("Invalid membership tag");
        }
        return Err(TranscriptTestVectorError::MembershipTagVerificationError);
    }
    let commit: MlsPlaintext = commit
        .verify(&crypto, &credential)
        .expect("Invalid signature on MlsPlaintext commit");

    //let my_confirmation_tag = confirmation_key.tag(&confirmed_transcript_hash_before);
    let confirmed_transcript_hash_after =
        hex_to_bytes(&test_vector.confirmed_transcript_hash_after);

    let my_confirmation_tag = confirmation_key.tag(&crypto, &confirmed_transcript_hash_after);
    if &my_confirmation_tag
        != commit
            .confirmation_tag()
            .expect("Confirmation tag is missing")
    {
        log::error!("  Confirmation tag mismatch");
        log::debug!("    Computed: {:x?}", my_confirmation_tag);
        log::debug!("    Expected: {:x?}", commit.confirmation_tag().unwrap());
        if cfg!(test) {
            panic!("Invalid confirmation tag");
        }
        return Err(TranscriptTestVectorError::ConfirmationTagMismatch);
    }

    // Compute new transcript hashes.
    let my_confirmed_transcript_hash_after = update_confirmed_transcript_hash(
        ciphersuite,
        &crypto,
        &MlsPlaintextCommitContent::try_from(&commit).unwrap(),
        &interim_transcript_hash_before,
    )
    .expect("Error updating confirmed transcript hash");
    if my_confirmed_transcript_hash_after != confirmed_transcript_hash_after {
        log::debug!("  Confirmed transcript hash mismatch");
        log::debug!("    Got:      {:x?}", my_confirmed_transcript_hash_after);
        log::debug!("    Expected: {:x?}", confirmed_transcript_hash_after);
        if cfg!(test) {
            panic!("Confirmed transcript hash mismatch");
        }
        return Err(TranscriptTestVectorError::ConfirmedTranscriptHashMismatch);
    }

    let interim_transcript_hash_after = hex_to_bytes(&test_vector.interim_transcript_hash_after);

    let my_interim_transcript_hash_after = update_interim_transcript_hash(
        ciphersuite,
        &crypto,
        &MlsPlaintextCommitAuthData::try_from(&commit).unwrap(),
        &my_confirmed_transcript_hash_after,
    )
    .expect("Error updating interim transcript hash");
    if my_interim_transcript_hash_after != interim_transcript_hash_after {
        log::debug!("  Interim transcript hash mismatch");
        log::debug!("    Got:      {:x?}", my_interim_transcript_hash_after);
        log::debug!("    Expected: {:x?}", interim_transcript_hash_after);
        if cfg!(test) {
            panic!("Interim transcript hash mismatch");
        }
        return Err(TranscriptTestVectorError::InterimTranscriptHashMismatch);
    }
    log::debug!("  Finished transcript test vector verification");
    Ok(())
}

#[test]
fn read_test_vectors() {
    let tests: Vec<TranscriptTestVector> = read("test_vectors/kat_transcripts.json");

    for test_vector in tests {
        match run_test_vector(test_vector) {
            Ok(_) => {}
            Err(e) => panic!("Error while checking transcript test vector.\n{:?}", e),
        }
    }

    // FIXME: change test vector spec. See https://github.com/mlswg/mls-implementations/pull/47
    // // mlspp test vectors
    // let tv_files = [
    //     "test_vectors/mlspp/mlspp_transcript_1.json",
    //     "test_vectors/mlspp/mlspp_transcript_2.json",
    //     "test_vectors/mlspp/mlspp_transcript_3.json",
    // ];
    // for &tv_file in tv_files.iter() {
    //     let tv: TranscriptTestVector = read(tv_file);
    //     run_test_vector(tv).expect("Error while checking key schedule test vector.");
    // }
}

implement_error! {
    pub enum TranscriptTestVectorError {
        MembershipTagVerificationError = "Membership tag could not be verified.",
        GroupContextMismatch = "The group context does not match",
        ConfirmationTagMismatch = "The computed confirmation tag doesn't match the one in the test vector.",
        ConfirmedTranscriptHashMismatch = "The computed transcript hash doesn't match the one in the test vector.",
        InterimTranscriptHashMismatch = "The computed interim transcript hash doesn't match the one in the test vector.",
    }
}
