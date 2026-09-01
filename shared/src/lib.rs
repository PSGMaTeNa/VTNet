use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Domain Separator for the cryptographic handshake to prevent replay and context attacks.
pub const AUTH_DOMAIN_SEPARATOR: &[u8] = b"VTNET_AUTH_v1";
pub const AUTH_NONCE_BYTES: usize = 32;

/// Builds the canonical byte sequence signed by the client and verified by the server.
///
/// Layout: domain separator | protocol version (u16 big-endian) | nonce (32 bytes) |
/// server timestamp (u64 big-endian) | client public key (32 bytes).
pub fn auth_signature_payload(
    protocol_version: u16,
    nonce: [u8; AUTH_NONCE_BYTES],
    server_timestamp: u64,
    client_uid: UserUid,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(
        AUTH_DOMAIN_SEPARATOR.len()
            + std::mem::size_of::<u16>()
            + AUTH_NONCE_BYTES
            + std::mem::size_of::<u64>()
            + client_uid.as_bytes().len(),
    );

    payload.extend_from_slice(AUTH_DOMAIN_SEPARATOR);
    payload.extend_from_slice(&protocol_version.to_be_bytes());
    payload.extend_from_slice(&nonce);
    payload.extend_from_slice(&server_timestamp.to_be_bytes());
    payload.extend_from_slice(client_uid.as_bytes());
    payload
}

/// Type-safe wrapper for an Ed25519 Public Key, enforced as a 32-byte array.
/// Serializes to a compact Base64 string
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UserUid([u8; 32]);

impl Serialize for UserUid {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(self.0))
    }
}

impl<'de> Deserialize<'de> for UserUid {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let decoded = STANDARD.decode(encoded).map_err(D::Error::custom)?;
        let bytes: [u8; 32] = decoded
            .try_into()
            .map_err(|_| D::Error::custom("user UID must decode to exactly 32 bytes"))?;

        Ok(Self(bytes))
    }
}

impl UserUid {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Validated WebRTC Session Description Type.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SdpType {
    Offer,
    Answer,
}

/// Öffentlich sichere Fehlergründe: keine internen Prüfdetails preisgeben.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthFailureReason {
    InvalidCredentials,
    UnsupportedProtocolVersion,
    RequestExpired,
    RateLimited,
}

pub const MAX_TEXT_CONTENT_BYTES: usize = 4 * 1024;

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct RoomTextContent(pub String);

impl RoomTextContent {
    pub fn new(value: String) -> Result<Self, &'static str> {
        if value.trim().is_empty() {
            return Err("text message must not be empty");
        }

        if value.len() > MAX_TEXT_CONTENT_BYTES {
            return Err("text message exceeds 4 KiB");
        }

        Ok(Self(value))
    }
}

impl<'de> Deserialize<'de> for RoomTextContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

// =========================================================================
// CLIENT MESSAGES (What the client is allowed to send to the server)
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Step 1: Client initiates handshake with protocol version and their public key.
    AuthIdentify {
        protocol_version: u16,
        client_uid: UserUid,
    },
    
    /// Step 2: Client returns the cryptographic proof.
    AuthResponse {
        /// Base64 Ed25519 signature of the canonical auth_signature_payload bytes.
        signature_base64: String,
    },
    
    /// Client joins a voice room and its bound text chat. The server automatically
    /// leaves any previously active voice room; joining the current room is a no-op.
    JoinVoiceRoom {
        room_id: Uuid,
    },
    
    /// Client leaves the current room.
    LeaveVoiceRoom {
        room_id: Uuid,
    },
    
    /// Client sends a text message. 
    /// KORREKTUR: No sender_uid or sender_name here! Server injects them from session memory.
    SendTextMessage {
        room_id: Uuid,
        text_content: RoomTextContent,
    },
    
    /// Client sends WebRTC SDP to a specific peer.
    /// KORREKTUR: No sender_uid. Server appends it before forwarding.
    SendWebRtcSdp {
        room_id: Uuid,
        target_uid: UserUid,
        sdp_type: SdpType,
        sdp_raw: String,
    },
    
    /// Client sends an ICE Candidate to a specific peer.
    SendWebRtcIce {
        room_id: Uuid,
        target_uid: UserUid,
        /// KORREKTUR: Strongly typed JSON object instead of a loose string
        candidate: serde_json::Value, 
    },
}

// =========================================================================
// SERVER MESSAGES (What the server broadcasts or replies to clients)
// =========================================================================

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "event", content = "payload", rename_all = "snake_case")]
pub enum ServerMessage {
    /// Step 1: Server sends the 32-byte nonce challenge.
    AuthChallenge {
        /// Base64 encoded 32 random bytes
        nonce_base64: String,
        /// Server time to detect extreme clock drifts (though validation relies on ephemeral nonce)
        server_timestamp: u64,
    },
    
    /// Step 2: Server acknowledges valid login and echoes verified session details.
    AuthSuccess {
        verified_uid: UserUid,
        assigned_name: String,
        server_name: String,
    },
    
    /// Server signals that the handshake failed.
    AuthFailure {
        reason: AuthFailureReason,
    },

    /// Broadcast: A new peer has joined the room. Existing clients must now initiate WebRTC.
    PeerJoinedRoom {
        room_id: Uuid,
        peer_uid: UserUid,
        peer_name: String,
    },

    /// Broadcast: A peer disconnected or left.
    PeerLeftRoom {
        room_id: Uuid,
        peer_uid: UserUid,
    },

    /// Broadcast: Real-time text message forwarded to room members.
    /// KORREKTUR: Authenticated metadata injected safely by the server core.
    BroadcastTextMessage {
        room_id: Uuid,
        sender_uid: UserUid,
        sender_name: String,
        text_content: RoomTextContent,
        server_timestamp: u64,
    },

    /// Forwarded: WebRTC SDP configuration from a peer.
    /// KORREKTUR: Contains the server-verified sender_uid so the receiver knows who it's from.
    ForwardWebRtcSdp {
        room_id: Uuid,
        sender_uid: UserUid,
        sdp_type: SdpType,
        sdp_raw: String,
    },

    /// Forwarded: WebRTC ICE Candidate from a peer.
    ForwardWebRtcIce {
        room_id: Uuid,
        sender_uid: UserUid,
        candidate: serde_json::Value,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_uid_serializes_as_base64_string_and_round_trips() {
        let user_uid = UserUid([42; 32]);

        let json = serde_json::to_string(&user_uid).unwrap();
        let deserialized: UserUid = serde_json::from_str(&json).unwrap();

        assert_eq!(json, format!("\"{}\"", STANDARD.encode([42; 32])));
        assert_eq!(deserialized, user_uid);
    }

    #[test]
    fn user_uid_rejects_invalid_base64() {
        let result = serde_json::from_str::<UserUid>("\"not valid base64!\"");

        assert!(result.is_err());
    }

    #[test]
    fn user_uid_rejects_invalid_byte_length() {
        let encoded = STANDARD.encode([0; 31]);
        let result = serde_json::from_str::<UserUid>(&format!("\"{encoded}\""));

        assert!(result.is_err());
    }

    #[test]
    fn room_text_content_deserializes_valid_text() {
        let text_content: RoomTextContent = serde_json::from_str("\"Hello, VTNet!\"").unwrap();

        assert_eq!(text_content, RoomTextContent("Hello, VTNet!".to_owned()));
    }

    #[test]
    fn room_text_content_rejects_whitespace_only_text() {
        let result = serde_json::from_str::<RoomTextContent>("\"   \\\n\\t\"");

        assert!(result.is_err());
    }

    #[test]
    fn room_text_content_rejects_text_larger_than_four_kib() {
        let oversized_text = "a".repeat(MAX_TEXT_CONTENT_BYTES + 1);
        let json = serde_json::to_string(&oversized_text).unwrap();
        let result = serde_json::from_str::<RoomTextContent>(&json);

        assert!(result.is_err());
    }

    #[test]
    fn auth_signature_payload_has_canonical_binary_layout() {
        let nonce = [0xAA; AUTH_NONCE_BYTES];
        let user_uid = UserUid([0xBB; 32]);
        let payload = auth_signature_payload(0x0102, nonce, 0x0102_0304_0506_0708, user_uid);

        let mut expected = AUTH_DOMAIN_SEPARATOR.to_vec();
        expected.extend_from_slice(&[0x01, 0x02]);
        expected.extend_from_slice(&nonce);
        expected.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        expected.extend_from_slice(&user_uid.0);

        assert_eq!(payload, expected);
    }
}
