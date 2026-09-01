use shared::{ClientMessage, ServerMessage};

pub const MAX_WEBSOCKET_TEXT_BYTES: usize = 64 * 1024;

/// Indicates why a WebSocket text frame cannot be processed as a protocol message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WebSocketMessageError {
    MessageTooLarge,
    InvalidProtocolMessage,
}

/// Decodes a bounded WebSocket text frame into a client protocol message.
pub fn decode_client_message(payload: &str) -> Result<ClientMessage, WebSocketMessageError> {
    if payload.len() > MAX_WEBSOCKET_TEXT_BYTES {
        return Err(WebSocketMessageError::MessageTooLarge);
    }

    serde_json::from_str(payload).map_err(|_| WebSocketMessageError::InvalidProtocolMessage)
}

/// Encodes a server protocol message for a WebSocket text frame.
pub fn encode_server_message(message: &ServerMessage) -> Result<String, serde_json::Error> {
    serde_json::to_string(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::UserUid;

    #[test]
    fn decodes_valid_client_protocol_json() {
        let message = ClientMessage::AuthIdentify {
            protocol_version: 1,
            client_uid: UserUid::from_bytes([1; 32]),
        };
        let json = serde_json::to_string(&message).unwrap();

        let decoded = decode_client_message(&json).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn rejects_invalid_protocol_json() {
        let result = decode_client_message("{ not valid json }");

        assert_eq!(result, Err(WebSocketMessageError::InvalidProtocolMessage));
    }

    #[test]
    fn rejects_text_frames_larger_than_the_transport_limit() {
        let payload = "a".repeat(MAX_WEBSOCKET_TEXT_BYTES + 1);

        let result = decode_client_message(&payload);

        assert_eq!(result, Err(WebSocketMessageError::MessageTooLarge));
    }
}