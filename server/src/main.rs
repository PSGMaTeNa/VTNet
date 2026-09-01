use std::{collections::HashMap, sync::Arc, time::{Instant, SystemTime, UNIX_EPOCH}};

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
    config::ServerConfig,
    database::Database,
    rooms::{RoomRepository, VoiceRoomLookup},
    transport::{decode_client_message, encode_server_message},
    PermissionEvaluator, RoomAction, RoomRegistry, Session, VoiceRoomTransition,
};
use shared::{ClientMessage, ServerMessage, UserUid, VoiceRoomJoinFailureReason, AUTH_NONCE_BYTES};
use tokio::sync::{mpsc, Mutex};

#[derive(Default)]
struct AllowAllPermissions;

impl PermissionEvaluator for AllowAllPermissions {
    fn effective_power(&self, _: UserUid, _: RoomAction) -> u16 {
        1
    }

    fn required_power(&self, _: uuid::Uuid, _: RoomAction) -> u16 {
        0
    }
}

/// Runtime-only state shared by all currently connected WebSocket clients.
struct ServerState {
    server_name: String,
    database: Database,
    rooms: RoomRegistry,
    connections: HashMap<UserUid, mpsc::UnboundedSender<ServerMessage>>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ServerConfig::from_environment()?;
    let mut database = Database::open(&config.database_path)?;
    database.initialize_schema()?;
    database.set_server_name(&config.server_name)?;
    if let Some(administrator) = config.initial_administrator {
        database.bootstrap_administrator(administrator.user_uid, &administrator.display_name)?;
    }
    if let Some(room_name) = &config.initial_ram_voice_room_name {
        let room = RoomRepository::new(database.connection())
            .ensure_initial_ram_voice_room(room_name)?;
        println!("Initial RAM voice room '{}' is available as {}", room.name, room.room_id);
    }

    let state = Arc::new(Mutex::new(ServerState {
        server_name: config.server_name,
        database,
        rooms: RoomRegistry::default(),
        connections: HashMap::new(),
    }));
    let app = Router::new().route("/ws", get(websocket_handler)).with_state(state);
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;

    println!("VTNet server listening on ws://{}/ws", config.bind_address);
    axum::serve(listener, app).await?;

    Ok(())
}

async fn websocket_handler(
    websocket: WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<Mutex<ServerState>>>,
) -> impl IntoResponse {
    websocket.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<Mutex<ServerState>>) {
    let server_name = state.lock().await.server_name.clone();
    let mut authenticator = ConnectionAuthenticator::new(server_name);
    let (mut socket_sender, mut socket_receiver) = socket.split();
    let (outbound_sender, mut outbound_receiver) = mpsc::unbounded_channel();

    let writer = tokio::spawn(async move {
        while let Some(message) = outbound_receiver.recv().await {
            let Ok(payload) = encode_server_message(&message) else {
                return;
            };

            if socket_sender.send(Message::Text(payload.into())).await.is_err() {
                return;
            }
        }
    });

    let mut session = None;
    let mut is_registered = false;

    while let Some(Ok(frame)) = socket_receiver.next().await {
        let Message::Text(payload) = frame else {
            if matches!(frame, Message::Close(_)) {
                break;
            }

            continue;
        };

        let Ok(message) = decode_client_message(&payload) else {
            break;
        };

        if let Some(session) = session.as_mut() {
            handle_authenticated_message(&state, session, message, unix_timestamp_millis()).await;
            continue;
        }

        let mut nonce = [0; AUTH_NONCE_BYTES];
        OsRng.fill_bytes(&mut nonce);
        let result = authenticator.handle_message(message, nonce, unix_timestamp_millis(), Instant::now());
        let server_message = match result {
            AuthenticationResult::Reply(message) | AuthenticationResult::Authenticated(message) => message,
        };

        if let Some(authenticated_session) = authenticator.take_authenticated_session() {
            let mut state = state.lock().await;
            // MVP permits one active WebSocket connection for each cryptographic identity.
            if state.connections.contains_key(&authenticated_session.user_uid()) {
                let _ = outbound_sender.send(ServerMessage::AuthFailure {
                    reason: shared::AuthFailureReason::InvalidCredentials,
                });
                break;
            }

            state
                .connections
                .insert(authenticated_session.user_uid(), outbound_sender.clone());
            session = Some(authenticated_session);
            is_registered = true;
        }

        let _ = outbound_sender.send(server_message);
    }

    if is_registered && let Some(session) = session {
        disconnect_session(&state, &session).await;
    }
    drop(outbound_sender);
    let _ = writer.await;
}

async fn handle_authenticated_message(
    state: &Arc<Mutex<ServerState>>,
    session: &mut Session,
    message: ClientMessage,
    server_timestamp: u64,
) {
    let mut state = state.lock().await;
    match message {
        ClientMessage::JoinVoiceRoom { room_id } => {
            let room = RoomRepository::new(state.database.connection()).validate_voice_room(room_id);
            let failure_reason = match room {
                Ok(VoiceRoomLookup::Found) => None,
                Ok(VoiceRoomLookup::NotVoiceRoom) => Some(VoiceRoomJoinFailureReason::NotAVoiceRoom),
                Ok(VoiceRoomLookup::NotFound) => Some(VoiceRoomJoinFailureReason::RoomNotFound),
                Err(_) => Some(VoiceRoomJoinFailureReason::Unavailable),
            };
            if let Some(reason) = failure_reason {
                let _ = state.connections.get(&session.user_uid()).map(|sender| sender.send(
                    ServerMessage::VoiceRoomJoinFailure { room_id, reason }
                ));
                return;
            }

            // Capture recipients before the transition removes the previous membership.
            let leaving_recipients = session.active_voice_room_id().map(|previous_room_id| {
                state.rooms.members(previous_room_id).into_iter().flatten()
                    .copied().filter(|user_uid| *user_uid != session.user_uid()).collect::<Vec<_>>()
            }).unwrap_or_default();
            let joining_recipients = state.rooms.members(room_id).into_iter().flatten()
                .copied().filter(|user_uid| *user_uid != session.user_uid()).collect::<Vec<_>>();

            let Ok(transition) = state.rooms.join_voice_room(session, room_id, &AllowAllPermissions) else {
                return;
            };
            if let VoiceRoomTransition::Switched { from_room_id, .. } = transition {
                send_to_users(&state, leaving_recipients, ServerMessage::PeerLeftRoom { room_id: from_room_id, peer_uid: session.user_uid() });
            }
            if matches!(transition, VoiceRoomTransition::Joined { .. } | VoiceRoomTransition::Switched { .. }) {
                let _ = state.connections.get(&session.user_uid()).map(|sender| sender.send(
                    ServerMessage::VoiceRoomJoined { room_id }
                ));
                send_to_users(&state, joining_recipients, ServerMessage::PeerJoinedRoom {
                    room_id,
                    peer_uid: session.user_uid(),
                    peer_name: session.display_name().to_owned(),
                });
            }
        }
        ClientMessage::LeaveVoiceRoom { room_id } => {
            let recipients = state.rooms.members(room_id).into_iter().flatten()
                .copied().filter(|user_uid| *user_uid != session.user_uid()).collect::<Vec<_>>();
            if let VoiceRoomTransition::Left { .. } = state.rooms.leave_voice_room(session, room_id) {
                let _ = state.connections.get(&session.user_uid()).map(|sender| sender.send(
                    ServerMessage::VoiceRoomLeft { room_id }
                ));
                send_to_users(&state, recipients, ServerMessage::PeerLeftRoom { room_id, peer_uid: session.user_uid() });
            }
        }
        room_message => {
            if let Ok(routed) = state.rooms.route_client_room_message(
                session, room_message, server_timestamp, &AllowAllPermissions,
            ) {
                send_to_users(&state, routed.recipients().iter().copied(), routed.message().clone());
            }
        }
    }
}

async fn disconnect_session(state: &Arc<Mutex<ServerState>>, session: &Session) {
    let mut state = state.lock().await;
    state.connections.remove(&session.user_uid());
    let Some(room_id) = session.active_voice_room_id() else {
        return;
    };
    // Notify remaining peers before removing the disconnecting session from the room.
    let recipients = state.rooms.members(room_id).into_iter().flatten()
        .copied().filter(|user_uid| *user_uid != session.user_uid()).collect::<Vec<_>>();
    if let VoiceRoomTransition::Left { .. } = state.rooms.leave_voice_room(&mut session.clone(), room_id) {
        send_to_users(&state, recipients, ServerMessage::PeerLeftRoom { room_id, peer_uid: session.user_uid() });
    }
}

fn send_to_users(
    state: &ServerState,
    recipients: impl IntoIterator<Item = UserUid>,
    message: ServerMessage,
) {
    for recipient in recipients {
        if let Some(sender) = state.connections.get(&recipient) {
            let _ = sender.send(message.clone());
        }
    }
}

fn unix_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis().min(u64::MAX as u128) as u64)
}
