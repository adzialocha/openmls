use crate::ciphersuite::{signable::*, *};
use crate::config::ProtocolVersion;
use crate::extensions::*;
use crate::group::*;
use crate::schedule::psk::PreSharedKeys;
use crate::schedule::JoinerSecret;
use crate::tree::{index::*, *};

use openmls_traits::types::HpkeCiphertext;

#[cfg(any(feature = "test-utils", test))]
use openmls_traits::OpenMlsCryptoProvider;

use serde::{Deserialize, Serialize};

mod codec;

pub mod errors;
pub(crate) mod proposals;
pub(crate) mod public_group_state;

pub use codec::*;
pub use errors::*;
use proposals::*;
use tls_codec::{
    Serialize as TlsSerializeTrait, TlsByteVecU32, TlsByteVecU8, TlsDeserialize, TlsSerialize,
    TlsSize, TlsVecU32,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
use crate::credentials::{CredentialBundle, CredentialError};

#[cfg(any(feature = "test-utils", test))]
use crate::schedule::{
    psk::{ExternalPsk, Psk, PskType::External},
    PreSharedKeyId,
};

/// Welcome Messages
///
/// > 11.2.2. Welcoming New Members
///
/// ```text
/// struct {
///   ProtocolVersion version = mls10;
///   CipherSuite cipher_suite;
///   EncryptedGroupSecrets secrets<0..2^32-1>;
///   opaque encrypted_group_info<1..2^32-1>;
/// } Welcome;
/// ```
#[derive(Clone, Debug, PartialEq, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct Welcome {
    version: ProtocolVersion,
    cipher_suite: CiphersuiteName,
    pub(crate) secrets: TlsVecU32<EncryptedGroupSecrets>,
    pub(crate) encrypted_group_info: TlsByteVecU32,
}

/// EncryptedGroupSecrets
///
/// > 11.2.2. Welcoming New Members
///
/// ```text
/// struct {
///   opaque key_package_hash<1..255>;
///   HPKECiphertext encrypted_group_secrets;
/// } EncryptedGroupSecrets;
/// ```
#[derive(Clone, Debug, PartialEq, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct EncryptedGroupSecrets {
    pub key_package_hash: TlsByteVecU8,
    pub encrypted_group_secrets: HpkeCiphertext,
}

impl Welcome {
    /// Create a new welcome message from the provided data.
    /// Note that secrets and the encrypted group info are consumed.
    pub(crate) fn new(
        version: ProtocolVersion,
        cipher_suite: &'static Ciphersuite,
        secrets: Vec<EncryptedGroupSecrets>,
        encrypted_group_info: Vec<u8>,
    ) -> Self {
        Self {
            version,
            cipher_suite: cipher_suite.name(),
            secrets: secrets.into(),
            encrypted_group_info: encrypted_group_info.into(),
        }
    }

    /// Get a reference to the ciphersuite in this Welcome message.
    pub(crate) fn ciphersuite(&self) -> CiphersuiteName {
        self.cipher_suite
    }

    /// Get a reference to the encrypted group secrets in this Welcome message.
    pub fn secrets(&self) -> &[EncryptedGroupSecrets] {
        self.secrets.as_slice()
    }

    /// Get a reference to the encrypted group info.
    pub(crate) fn encrypted_group_info(&self) -> &[u8] {
        self.encrypted_group_info.as_slice()
    }

    /// Get a reference to the protocol version in the `Welcome`.
    pub(crate) fn version(&self) -> &ProtocolVersion {
        &self.version
    }

    /// Set the welcome's encrypted group info.
    #[cfg(test)]
    pub fn set_encrypted_group_info(&mut self, encrypted_group_info: Vec<u8>) {
        self.encrypted_group_info = encrypted_group_info.into();
    }
}

#[derive(
    Debug, PartialEq, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize,
)]
pub struct Commit {
    pub(crate) proposals: TlsVecU32<ProposalOrRef>,
    pub(crate) path: Option<UpdatePath>,
}

impl Commit {
    /// Returns `true` if the commit contains an update path. `false` otherwise.
    pub fn has_path(&self) -> bool {
        self.path.is_some()
    }

    #[cfg(any(feature = "test-utils", test))]
    pub fn path(&self) -> &Option<UpdatePath> {
        &self.path
    }
}

/// Confirmation tag field of MlsPlaintext. For type safety this is a wrapper
/// around a `Mac`.
#[derive(
    Debug, PartialEq, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize,
)]
pub struct ConfirmationTag(pub(crate) Mac);

#[derive(TlsDeserialize, TlsSerialize, TlsSize)]
pub(crate) struct GroupInfoPayload {
    group_id: GroupId,
    epoch: GroupEpoch,
    tree_hash: TlsByteVecU8,
    confirmed_transcript_hash: TlsByteVecU8,
    group_context_extensions: TlsVecU32<Extension>,
    other_extensions: TlsVecU32<Extension>,
    confirmation_tag: ConfirmationTag,
    signer_index: LeafIndex,
}

impl GroupInfoPayload {
    #[allow(clippy::too_many_arguments)] // TODO: #569 refactor GroupInfoPayload
    /// Create a new group info payload struct.
    pub(crate) fn new(
        group_id: GroupId,
        epoch: GroupEpoch,
        tree_hash: Vec<u8>,
        confirmed_transcript_hash: Vec<u8>,
        group_context_extensions: &[Extension],
        other_extensions: &[Extension],
        confirmation_tag: ConfirmationTag,
        signer_index: LeafIndex,
    ) -> Self {
        Self {
            group_id,
            epoch,
            tree_hash: tree_hash.into(),
            confirmed_transcript_hash: confirmed_transcript_hash.into(),
            group_context_extensions: group_context_extensions.into(),
            other_extensions: other_extensions.into(),
            confirmation_tag,
            signer_index,
        }
    }
}

impl Signable for GroupInfoPayload {
    type SignedOutput = GroupInfo;

    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.tls_serialize_detached()
    }
}

/// GroupInfo
///
/// The struct is split into the payload and the signature.
/// `GroupInfoPayload` holds the actual values, stored in `payload` here.
///
/// > 11.2.2. Welcoming New Members
///
/// ```text
/// struct {
///   opaque group_id<0..255>;
///   uint64 epoch;
///   opaque tree_hash<0..255>;
///   opaque confirmed_transcript_hash<0..255>;
///   Extension extensions<0..2^32-1>;
///   MAC confirmation_tag;
///   uint32 signer_index;
///   opaque signature<0..2^16-1>;
/// } GroupInfo;
/// ```
pub(crate) struct GroupInfo {
    payload: GroupInfoPayload,
    signature: Signature,
}

impl GroupInfo {
    /// Get the tree hash as byte slice.
    pub(crate) fn tree_hash(&self) -> &[u8] {
        self.payload.tree_hash.as_slice()
    }

    /// Get the signer index.
    pub(crate) fn signer_index(&self) -> LeafIndex {
        self.payload.signer_index
    }

    /// Get the group ID.
    pub(crate) fn group_id(&self) -> &GroupId {
        &self.payload.group_id
    }

    /// Get the epoch.
    pub(crate) fn epoch(&self) -> GroupEpoch {
        self.payload.epoch
    }

    /// Get the confirmed transcript hash.
    pub(crate) fn confirmed_transcript_hash(&self) -> &[u8] {
        self.payload.confirmed_transcript_hash.as_slice()
    }

    /// Get the confirmed tag.
    pub(crate) fn confirmation_tag(&self) -> &ConfirmationTag {
        &self.payload.confirmation_tag
    }

    /// Get other application extensions.
    pub(crate) fn other_extensions(&self) -> &[Extension] {
        self.payload.other_extensions.as_slice()
    }

    /// Get the [`GroupContext`] extensions.
    pub(crate) fn group_context_extensions(&self) -> &[Extension] {
        self.payload.group_context_extensions.as_slice()
    }

    /// Set the group info's other extensions.
    #[cfg(test)]
    pub(crate) fn set_other_extensions(&mut self, extensions: Vec<Extension>) {
        self.payload.other_extensions = extensions.into();
    }

    /// Re-sign the group info.
    #[cfg(test)]
    pub(crate) fn re_sign(
        self,
        credential_bundle: &CredentialBundle,
        backend: &impl OpenMlsCryptoProvider,
    ) -> Result<Self, CredentialError> {
        self.payload.sign(backend, credential_bundle)
    }
}

impl Verifiable for GroupInfo {
    fn unsigned_payload(&self) -> Result<Vec<u8>, tls_codec::Error> {
        self.payload.unsigned_payload()
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }
}

impl SignedStruct<GroupInfoPayload> for GroupInfo {
    fn from_payload(payload: GroupInfoPayload, signature: Signature) -> Self {
        Self { payload, signature }
    }
}

/// PathSecret
///
/// > 11.2.2. Welcoming New Members
///
/// ```text
/// struct {
///   opaque path_secret<1..255>;
/// } PathSecret;
/// ```
#[derive(Debug, Serialize, Deserialize, TlsSerialize, TlsDeserialize, TlsSize)]
#[cfg_attr(any(feature = "test-utils", test), derive(PartialEq, Clone))]
pub struct PathSecret {
    pub(crate) path_secret: Secret,
}

impl From<Secret> for PathSecret {
    fn from(path_secret: Secret) -> Self {
        Self { path_secret }
    }
}

/// GroupSecrets
///
/// > 11.2.2. Welcoming New Members
///
/// ```text
/// struct {
///   opaque joiner_secret<1..255>;
///   optional<PathSecret> path_secret;
///   optional<PreSharedKeys> psks;
/// } GroupSecrets;
/// ```
#[derive(TlsDeserialize, TlsSize)]
pub(crate) struct GroupSecrets {
    pub(crate) joiner_secret: JoinerSecret,
    pub(crate) path_secret: Option<PathSecret>,
    pub(crate) psks: PreSharedKeys,
}

#[derive(TlsSerialize, TlsSize)]
struct EncodedGroupSecrets<'a> {
    pub(crate) joiner_secret: &'a JoinerSecret,
    pub(crate) path_secret: Option<&'a PathSecret>,
    pub(crate) psks: &'a PreSharedKeys,
}

impl GroupSecrets {
    /// Create new encoded group secrets.
    pub(crate) fn new_encoded<'a>(
        joiner_secret: &JoinerSecret,
        path_secret: Option<&'a PathSecret>,
        psks: &'a PreSharedKeys,
    ) -> Result<Vec<u8>, tls_codec::Error> {
        EncodedGroupSecrets {
            joiner_secret,
            path_secret,
            psks,
        }
        .tls_serialize_detached()
    }

    /// Set the config for the secrets, i.e. cipher suite and MLS version.
    pub(crate) fn config(
        mut self,
        ciphersuite: &'static Ciphersuite,
        mls_version: ProtocolVersion,
    ) -> GroupSecrets {
        self.joiner_secret.config(ciphersuite, mls_version);
        if let Some(s) = &mut self.path_secret {
            s.path_secret.config(ciphersuite, mls_version);
        }
        self
    }

    #[cfg(any(feature = "test-utils", test))]
    pub fn random_encoded(
        ciphersuite: &'static Ciphersuite,
        backend: &impl OpenMlsCryptoProvider,
        version: ProtocolVersion,
    ) -> Result<Vec<u8>, tls_codec::Error> {
        use openmls_traits::random::OpenMlsRand;

        let psk_id = PreSharedKeyId::new(
            External,
            Psk::External(ExternalPsk::new(
                backend
                    .rand()
                    .random_vec(ciphersuite.hash_length())
                    .unwrap(),
            )),
            backend
                .rand()
                .random_vec(ciphersuite.hash_length())
                .unwrap(),
        );
        let psks = PreSharedKeys {
            psks: vec![psk_id].into(),
        };

        GroupSecrets::new_encoded(
            &JoinerSecret::random(ciphersuite, backend, version),
            Some(&PathSecret {
                path_secret: Secret::random(ciphersuite, backend, version),
            }),
            &psks,
        )
    }
}
