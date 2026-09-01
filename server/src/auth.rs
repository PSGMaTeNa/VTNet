use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use ed25519_dalek::{Signature, VerifyingKey};
use shared::{
    auth_signature_payload, AuthFailureReason, ClientMessage, ServerMessage, UserUid,
    AUTH_NONCE_BYTES,
};

use crate::Session;

pub const PROTOCOL_VERSION: u16 = 1;
pub const AUTH_CHALLENGE_TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthChallenge {
    client_uid: UserUid,
    protocol_version: u16,
    nonce: [u8; AUTH_NONCE_BYTES],
    server_timestamp: u64,
    issued_at: Instant,
}

/// Tracks the authentication state of one WebSocket connection.
#[derive(Debug)]
pub struct ConnectionAuthenticator {
    server_name: String,
    state: AuthenticationState,
}

#[derive(Debug)]
enum AuthenticationState {
    Connected,
    ChallengeIssued(AuthChallenge),
    Authenticated(Session),
}

/// The server reply produced by one authentication request.
#[derive(Debug, PartialEq)]
pub enum AuthenticationResult {
    Reply(ServerMessage),
    Authenticated(ServerMessage),
}

impl ConnectionAuthenticator {
    pub fn new(server_name: String) -> Self {
        Self {
            server_name,
            state: AuthenticationState::Connected,
        }
    }

    pub fn authenticated_session(&self) -> Option<&Session> {
        match &self.state {
            AuthenticationState::Authenticated(session) => Some(session),
            _ => None,
        }
    }

    /// Transfers the authenticated session to the connection runtime.
    pub fn take_authenticated_session(&mut self) -> Option<Session> {
        let AuthenticationState::Authenticated(_) = self.state else {
            return None;
        };

        let previous_state = std::mem::replace(&mut self.state, AuthenticationState::Connected);
        let AuthenticationState::Authenticated(session) = previous_state else {
            unreachable!("authenticated state was checked before replacement");
        };

        Some(session)
    }

    pub fn handle_message(
        &mut self,
        message: ClientMessage,
        nonce: [u8; AUTH_NONCE_BYTES],
        server_timestamp: u64,
        now: Instant,
    ) -> AuthenticationResult {
        match (&self.state, message) {
            (
                AuthenticationState::Connected,
                ClientMessage::AuthIdentify {
                    protocol_version,
                    client_uid,
                },
            ) if protocol_version == PROTOCOL_VERSION => {
                self.state = AuthenticationState::ChallengeIssued(AuthChallenge {
                    client_uid,
                    protocol_version,
                    nonce,
                    server_timestamp,
                    issued_at: now,
                });

                AuthenticationResult::Reply(ServerMessage::AuthChallenge {
                    nonce_base64: STANDARD.encode(nonce),
                    server_timestamp,
                })
            }
            (AuthenticationState::Connected, ClientMessage::AuthIdentify { .. }) => {
                Self::failure(AuthFailureReason::UnsupportedProtocolVersion)
            }
            (AuthenticationState::ChallengeIssued(challenge), ClientMessage::AuthResponse { signature_base64 }) => {
                let challenge = *challenge;
                self.state = AuthenticationState::Connected;

                if now.duration_since(challenge.issued_at) > AUTH_CHALLENGE_TTL {
                    return Self::failure(AuthFailureReason::RequestExpired);
                }

                if !Self::verify_signature(challenge, &signature_base64) {
                    return Self::failure(AuthFailureReason::InvalidCredentials);
                }

                let assigned_name = Self::default_display_name(challenge.client_uid);
                self.state = AuthenticationState::Authenticated(Session::new(
                    challenge.client_uid,
                    assigned_name.clone(),
                ));

                AuthenticationResult::Authenticated(ServerMessage::AuthSuccess {
                    verified_uid: challenge.client_uid,
                    assigned_name,
                    server_name: self.server_name.clone(),
                })
            }
            _ => Self::failure(AuthFailureReason::InvalidCredentials),
        }
    }

    fn verify_signature(challenge: AuthChallenge, signature_base64: &str) -> bool {
        let Ok(signature_bytes) = STANDARD.decode(signature_base64) else {
            return false;
        };
        let Ok(signature) = Signature::from_slice(&signature_bytes) else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(challenge.client_uid.as_bytes()) else {
            return false;
        };

        let payload = auth_signature_payload(
            challenge.protocol_version,
            challenge.nonce,
            challenge.server_timestamp,
            challenge.client_uid,
        );

        verifying_key.verify_strict(&payload, &signature).is_ok()
    }

    fn default_display_name(user_uid: UserUid) -> String {
        let [first, second, third, fourth, ..] = *user_uid.as_bytes();
        format!("Guest-{first:02x}{second:02x}{third:02x}{fourth:02x}")
    }

    fn failure(reason: AuthFailureReason) -> AuthenticationResult {
        AuthenticationResult::Reply(ServerMessage::AuthFailure { reason })
    }
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    fn identify_message(signing_key: &SigningKey) -> ClientMessage {
        ClientMessage::AuthIdentify {
            protocol_version: PROTOCOL_VERSION,
            client_uid: UserUid::from_bytes(signing_key.verifying_key().to_bytes()),
        }
    }

    #[test]
    fn supported_identify_issues_a_challenge() {
        let signing_key = SigningKey::from_bytes(&[1; 32]);
        let mut authenticator = ConnectionAuthenticator::new("VTNet".to_owned());
        let nonce = [2; AUTH_NONCE_BYTES];
        let timestamp = 123;

        let result = authenticator.handle_message(identify_message(&signing_key), nonce, timestamp, Instant::now());

        assert_eq!(
            result,
            AuthenticationResult::Reply(ServerMessage::AuthChallenge {
                nonce_base64: STANDARD.encode(nonce),
                server_timestamp: timestamp,
            })
        );
    }

    #[test]
    fn valid_response_authenticates_the_connection() {
        let signing_key = SigningKey::from_bytes(&[1; 32]);
        let user_uid = UserUid::from_bytes(signing_key.verifying_key().to_bytes());
        let mut authenticator = ConnectionAuthenticator::new("VTNet".to_owned());
        let nonce = [2; AUTH_NONCE_BYTES];
        let timestamp = 123;
        let now = Instant::now();
        authenticator.handle_message(identify_message(&signing_key), nonce, timestamp, now);
        let payload = auth_signature_payload(PROTOCOL_VERSION, nonce, timestamp, user_uid);
        let signature_base64 = STANDARD.encode(signing_key.sign(&payload).to_bytes());

        let result = authenticator.handle_message(
            ClientMessage::AuthResponse { signature_base64 },
            [0; AUTH_NONCE_BYTES],
            0,
            now,
        );

        assert_eq!(
            result,
            AuthenticationResult::Authenticated(ServerMessage::AuthSuccess {
                verified_uid: user_uid,
                assigned_name: "Guest-8a88e3dd".to_owned(),
                server_name: "VTNet".to_owned(),
            })
        );
        assert_eq!(authenticator.authenticated_session().unwrap().user_uid(), user_uid);
    }

    #[test]
    fn expired_challenge_rejects_the_response() {
        let signing_key = SigningKey::from_bytes(&[1; 32]);
        let mut authenticator = ConnectionAuthenticator::new("VTNet".to_owned());
        let now = Instant::now();
        authenticator.handle_message(identify_message(&signing_key), [2; AUTH_NONCE_BYTES], 123, now);

        let result = authenticator.handle_message(
            ClientMessage::AuthResponse {
                signature_base64: "invalid".to_owned(),
            },
            [0; AUTH_NONCE_BYTES],
            0,
            now + AUTH_CHALLENGE_TTL + Duration::from_millis(1),
        );

        assert_eq!(
            result,
            AuthenticationResult::Reply(ServerMessage::AuthFailure {
                reason: AuthFailureReason::RequestExpired,
            })
        );
    }
}