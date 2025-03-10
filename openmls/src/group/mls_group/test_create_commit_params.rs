use mls_group::{create_commit_params::CreateCommitParams, proposals::ProposalStore};
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::types::SignatureScheme;

use crate::credentials::CredentialType;

use super::*;

// Tests that the builder for CreateCommitParams works as expected
#[test]
fn build_create_commit_params() {
    let crypto = OpenMlsRustCrypto::default();

    let framing_parameters: FramingParameters =
        FramingParameters::new(&[1, 2, 3], WireFormat::MlsCiphertext);
    let credential_bundle: &CredentialBundle = &CredentialBundle::new(
        vec![4, 5, 6],
        CredentialType::Basic,
        SignatureScheme::ED25519,
        &crypto,
    )
    .expect("Could not create new CredentialBundle.");
    let proposal_store: &ProposalStore = &ProposalStore::new();
    let inline_proposals: Vec<Proposal> = vec![];
    let force_self_update: bool = true;
    let psk_fetcher_option: Option<PskFetcher> = None;

    let params = CreateCommitParams::builder()
        .framing_parameters(framing_parameters)
        .credential_bundle(credential_bundle)
        .proposal_store(proposal_store)
        .inline_proposals(inline_proposals.clone())
        .force_self_update(force_self_update)
        .psk_fetcher_option(psk_fetcher_option)
        .build();

    assert_eq!(params.framing_parameters(), &framing_parameters);
    assert_eq!(params.credential_bundle(), credential_bundle);
    assert_eq!(params.proposal_store(), proposal_store);
    assert_eq!(params.inline_proposals(), inline_proposals);
    assert_eq!(params.force_self_update(), force_self_update);
}
