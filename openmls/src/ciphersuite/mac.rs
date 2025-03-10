use super::*;

/// 9.2 Message framing
///
/// struct {
///     opaque mac_value<0..255>;
/// } MAC;
#[derive(Debug, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub(crate) struct Mac {
    pub(crate) mac_value: TlsByteVecU8,
}

impl PartialEq for Mac {
    // Constant time comparison.
    fn eq(&self, other: &Mac) -> bool {
        equal_ct(self.mac_value.as_slice(), other.mac_value.as_slice())
    }
}

impl Mac {
    /// HMAC-Hash(salt, IKM). For all supported ciphersuites this is the same
    /// HMAC that is also used in HKDF.
    /// Compute the HMAC on `salt` with key `ikm`.
    pub(crate) fn new(backend: &impl OpenMlsCryptoProvider, salt: &Secret, ikm: &[u8]) -> Self {
        Mac {
            mac_value: salt
                .hkdf_extract(
                    backend,
                    &Secret::from_slice(ikm, salt.mls_version, salt.ciphersuite),
                )
                .value
                .into(),
        }
    }
}
