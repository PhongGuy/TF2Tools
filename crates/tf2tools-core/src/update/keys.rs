//! Manifest signature checking: minisign public keys held in rotation slots, any one
//! of which may sign a release manifest.

use crate::error::UpdateCheckError;

/// The production minisign public keys (base64), any of which may sign a manifest.
/// Two slots for rotation; the first holds the release key (minisign key id 53867C793ACEB351).
pub const UPDATE_PUBLIC_KEYS: &[&str] =
    &["RWRRs846eXyGU3h3tVjuB++1Blcrbl9Js5eIwzQnKM7MDgTvPhPGCQul"];

/// Decoded public keys, tried in slot order when checking a signature.
#[derive(Debug, Clone)]
pub(crate) struct Keys(Vec<minisign_verify::PublicKey>);

impl Keys {
    /// BadKey when `keys` is empty or any entry, trimmed, is not a minisign public key;
    /// the message names the 1-based slot ("slot 2").
    pub(crate) fn decode(keys: &[&str]) -> Result<Keys, UpdateCheckError> {
        if keys.is_empty() {
            return Err(UpdateCheckError::BadKey {
                message: "no public key is configured".to_owned(),
            });
        }
        keys.iter()
            .enumerate()
            .map(|(index, key)| {
                minisign_verify::PublicKey::from_base64(key.trim()).map_err(|err| {
                    UpdateCheckError::BadKey {
                        message: format!("slot {}: {err}", index + 1),
                    }
                })
            })
            .collect::<Result<_, _>>()
            .map(Keys)
    }

    /// Ok when any key verifies `signature` (a .minisig file's bytes) over exactly `data`;
    /// otherwise BadSignature, including for non-UTF-8 or undecodable signatures.
    pub(crate) fn verify(&self, data: &[u8], signature: &[u8]) -> Result<(), UpdateCheckError> {
        let signature = std::str::from_utf8(signature)
            .ok()
            .and_then(|text| minisign_verify::Signature::decode(text).ok())
            .ok_or(UpdateCheckError::BadSignature)?;
        // Rotation: a key whose id does not match simply fails, so the first success wins.
        // Legacy (non-prehashed) signatures are refused.
        self.0
            .iter()
            .any(|key| key.verify(data, &signature, false).is_ok())
            .then_some(())
            .ok_or(UpdateCheckError::BadSignature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const DATA: &[u8] = br#"{"version":"6.0.1","platforms":{}}"#;

    fn pair() -> minisign::KeyPair {
        minisign::KeyPair::generate_unencrypted_keypair().unwrap()
    }

    fn public(pair: &minisign::KeyPair) -> String {
        pair.pk.to_base64()
    }

    fn sign(pair: &minisign::KeyPair, data: &[u8]) -> String {
        minisign::sign(Some(&pair.pk), &pair.sk, Cursor::new(data), None, None)
            .unwrap()
            .into_string()
    }

    fn bad_signature(result: Result<(), UpdateCheckError>) {
        assert!(
            matches!(result, Err(UpdateCheckError::BadSignature)),
            "{result:?}"
        );
    }

    #[test]
    fn a_signature_by_the_only_key_verifies() {
        let a = pair();
        let keys = Keys::decode(&[&public(&a)]).unwrap();
        keys.verify(DATA, sign(&a, DATA).as_bytes()).unwrap();
    }

    #[test]
    fn a_signature_by_the_second_slot_verifies() {
        let (a, b) = (pair(), pair());
        let keys = Keys::decode(&[&public(&a), &public(&b)]).unwrap();
        keys.verify(DATA, sign(&b, DATA).as_bytes()).unwrap();
        keys.verify(DATA, sign(&a, DATA).as_bytes()).unwrap();
    }

    #[test]
    fn a_signature_by_an_unknown_key_is_bad() {
        let (a, stranger) = (pair(), pair());
        let keys = Keys::decode(&[&public(&a)]).unwrap();
        bad_signature(keys.verify(DATA, sign(&stranger, DATA).as_bytes()));
    }

    #[test]
    fn a_tampered_body_is_bad() {
        let a = pair();
        let keys = Keys::decode(&[&public(&a)]).unwrap();
        let signature = sign(&a, DATA);
        let mut tampered = DATA.to_vec();
        tampered.push(b' ');
        bad_signature(keys.verify(&tampered, signature.as_bytes()));
    }

    #[test]
    fn a_garbled_empty_or_truncated_signature_is_bad() {
        let a = pair();
        let keys = Keys::decode(&[&public(&a)]).unwrap();
        let whole = sign(&a, DATA);
        let truncated: String = whole.lines().take(2).collect::<Vec<_>>().join("\n");
        for signature in [
            b"".as_slice(),
            b"not a signature",
            &[0xff, 0xfe],
            truncated.as_bytes(),
        ] {
            bad_signature(keys.verify(DATA, signature));
        }
    }

    #[test]
    fn no_keys_is_a_bad_key() {
        let err = Keys::decode(&[]).unwrap_err();
        assert!(matches!(err, UpdateCheckError::BadKey { .. }), "{err:?}");
    }

    #[test]
    fn an_undecodable_key_is_a_bad_key_naming_its_slot() {
        let good = public(&pair());
        match Keys::decode(&[&good, "not a key"]) {
            Err(UpdateCheckError::BadKey { message }) => {
                assert!(message.contains("slot 2"), "{message}")
            }
            other => panic!("expected BadKey, got {other:?}"),
        }
    }

    #[test]
    fn surrounding_whitespace_on_a_key_is_ignored() {
        let a = pair();
        let padded = format!("  {}\n", public(&a));
        let keys = Keys::decode(&[&padded]).unwrap();
        keys.verify(DATA, sign(&a, DATA).as_bytes()).unwrap();
    }

    #[test]
    fn the_production_slots_hold_at_most_two_decodable_keys() {
        // black_box keeps clippy's constant lints off a slot list whose length is a constant.
        let slots = std::hint::black_box(UPDATE_PUBLIC_KEYS);
        assert!(slots.len() <= 2, "{slots:?}");
        if !slots.is_empty() {
            Keys::decode(slots).unwrap();
        }
    }
}
