use std::sync::{LazyLock, Mutex};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use keyring::Entry;
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use shared::{auth_signature_payload, UserUid, AUTH_NONCE_BYTES};

const KEYRING_SERVICE: &str = "VTNet";
const KEYRING_ACCOUNT: &str = "ed25519_identity_seed_v1";
const ED25519_SEED_BYTES: usize = 32;
static IDENTITY_SEED: LazyLock<Mutex<Option<[u8; ED25519_SEED_BYTES]>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Serialize)]
pub struct IdentityInfo {
    pub user_uid: UserUid,
}

#[derive(Deserialize)]
pub struct SignAuthChallengeRequest {
    pub protocol_version: u16,
    pub nonce_base64: String,
    pub server_timestamp: u64,
}

/// Returns the public identity while retaining the private seed in the OS keyring.
#[tauri::command]
pub fn get_or_create_identity() -> Result<IdentityInfo, String> {
    let signing_key = load_or_create_signing_key()?;

    Ok(IdentityInfo {
        user_uid: UserUid::from_bytes(signing_key.verifying_key().to_bytes()),
    })
}

/// Signs only the canonical VTNet authentication payload, never arbitrary frontend data.
#[tauri::command]
pub fn sign_auth_challenge(request: SignAuthChallengeRequest) -> Result<String, String> {
    let nonce = decode_nonce(&request.nonce_base64)?;
    let signing_key = load_or_create_signing_key()?;
    let user_uid = UserUid::from_bytes(signing_key.verifying_key().to_bytes());
    let payload = auth_signature_payload(
        request.protocol_version,
        nonce,
        request.server_timestamp,
        user_uid,
    );

    Ok(STANDARD.encode(signing_key.sign(&payload).to_bytes()))
}

fn load_or_create_signing_key() -> Result<SigningKey, String> {
    // Keep one stable identity per app process, even if another development instance changes the keyring entry.
    let mut cached_seed = IDENTITY_SEED
        .lock()
        .map_err(|_| "identity initialization lock was poisoned".to_owned())?;
    if let Some(seed) = *cached_seed {
        return Ok(SigningKey::from_bytes(&seed));
    }

    let keyring_entry = Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
        .map_err(|error| format!("could not access the OS keyring: {error}"))?;

    let seed = match keyring_entry.get_password() {
        Ok(encoded_seed) => decode_seed(&encoded_seed)?,
        Err(keyring::Error::NoEntry) => {
            let mut seed = [0; ED25519_SEED_BYTES];
            OsRng.fill_bytes(&mut seed);
            keyring_entry
                .set_password(&STANDARD.encode(seed))
                .map_err(|error| format!("could not save identity in the OS keyring: {error}"))?;

            seed
        }
        Err(error) => return Err(format!("could not load identity from the OS keyring: {error}")),
    };

    *cached_seed = Some(seed);
    Ok(SigningKey::from_bytes(&seed))
}

fn decode_seed(encoded_seed: &str) -> Result<[u8; ED25519_SEED_BYTES], String> {
    let seed = STANDARD
        .decode(encoded_seed)
        .map_err(|_| "stored identity is not valid Base64".to_owned())?;
    seed
        .try_into()
        .map_err(|_| "stored identity has an invalid length".to_owned())
}

fn decode_nonce(encoded_nonce: &str) -> Result<[u8; AUTH_NONCE_BYTES], String> {
    let nonce = STANDARD
        .decode(encoded_nonce)
        .map_err(|_| "authentication nonce is not valid Base64".to_owned())?;

    nonce
        .try_into()
        .map_err(|_| "authentication nonce must contain exactly 32 bytes".to_owned())
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::VerifyingKey;

    use super::*;

    #[test]
    fn encoded_seed_restores_the_same_ed25519_identity() {
        let seed = [7; ED25519_SEED_BYTES];
        let restored_key = SigningKey::from_bytes(&decode_seed(&STANDARD.encode(seed)).unwrap());

        assert_eq!(restored_key.to_bytes(), seed);
    }

    #[test]
    fn nonce_decoder_rejects_an_incorrect_length() {
        let result = decode_nonce(&STANDARD.encode([0; AUTH_NONCE_BYTES - 1]));

        assert!(result.is_err());
    }

    #[test]
    fn auth_signature_can_be_verified_by_the_public_identity() {
        let signing_key = SigningKey::from_bytes(&[9; ED25519_SEED_BYTES]);
        let user_uid = UserUid::from_bytes(signing_key.verifying_key().to_bytes());
        let nonce = [4; AUTH_NONCE_BYTES];
        let payload = auth_signature_payload(1, nonce, 42, user_uid);
        let signature = signing_key.sign(&payload);
        let verifying_key = VerifyingKey::from_bytes(user_uid.as_bytes()).unwrap();

        assert!(verifying_key.verify_strict(&payload, &signature).is_ok());
    }
}