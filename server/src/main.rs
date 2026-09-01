use std::time::{Instant, SystemTime, UNIX_EPOCH};

use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures_util::{SinkExt, StreamExt};
use rand_core::{OsRng, RngCore};
use server::{
    auth::{AuthenticationResult, ConnectionAuthenticator},
    transport::{decode_client_message, encode_server_message},
};
use shared::AUTH_NONCE_BYTES;

const SERVER_NAME: &str = "VTNet";
const LISTEN_ADDRESS: &str = "127.0.0.1:3000";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = Router::new().route("/ws", get(websocket_handler));
    let listener = tokio::net::TcpListener::bind(LISTEN_ADDRESS).await?;

    println!("VTNet server listening on ws://{LISTEN_ADDRESS}/ws");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn websocket_handler(websocket: WebSocketUpgrade) -> impl IntoResponse {
    websocket.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    let mut authenticator = ConnectionAuthenticator::new(SERVER_NAME.to_owned());

    while let Some(Ok(frame)) = socket.next().await {
        let Message::Text(payload) = frame else {
            if matches!(frame, Message::Close(_)) {
                return;
            }

            continue;
        };

        let Ok(message) = decode_client_message(&payload) else {
            let _ = socket.close().await;
            return;
        };

        if authenticator.authenticated_session().is_some() {
            // Room routing is connected after the shared server state is added.
            let _ = socket.close().await;
            return;
        }

        let mut nonce = [0; AUTH_NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce);
        let result = authenticator.handle_message(message, nonce, unix_timestamp_millis(), Instant::now());
        let server_message = match result {
            AuthenticationResult::Reply(message) | AuthenticationResult::Authenticated(message) => message,
        };

        let Ok(payload) = encode_server_message(&server_message) else {
            let _ = socket.close().await;
            return;
        };

        if socket.send(Message::Text(payload.into())).await.is_err() {
            return;
        }
    }
}

fn unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis().min(u64::MAX as u128) as u64)
}
