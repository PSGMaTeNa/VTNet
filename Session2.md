# Session 2 - SQLite Room Management

## Goal

Replace the temporary in-memory assumptions with a persistent server structure. The server owns RAM voice-room definitions, user roles, and power requirements. Active voice presence and RAM text remain volatile and are never stored in SQLite.

`ram_voice` rooms are TeamSpeak-style real-time channels: they support WebRTC audio and, in a later phase, video and screen sharing. Their bound RAM text chat has no history and does not accept attachments. `e2ee_text` rooms are Discord-style persistent text channels: they have no voice presence or WebRTC signaling, but support encrypted text, images, files, and links. Access to `e2ee_text` rooms is granted through explicit room-to-role assignments; powers control administrative and fine-grained actions.

## Order

- [ ] **SQLite foundation**
  - [x] Add a SQLite dependency and a server-owned database configuration.
  - [x] Create migrations for `server_config`, `trusted_users`, `roles`, `user_roles`, `rooms`, `room_roles`, `role_powers`, and `room_power_requirements`.
  - [x] Seed the single server configuration row and an explicitly configured initial administrator identity during server setup.
  - [x] Add tests that verify schema creation and migration idempotency.

- [ ] **Persistent room definitions**
  - [ ] Model `ram_voice` and `e2ee_text` room types in a repository layer.
  - [ ] Keep `ram_voice` limited to ephemeral presence, RAM text, and current/future WebRTC media; never treat `e2ee_text` as a voice room.
  - [ ] Require `JoinVoiceRoom` to target an existing `ram_voice` room.
  - [ ] Reject joins to unknown rooms and persistent E2EE rooms without changing active presence.
  - [ ] Add tests for valid rooms, missing rooms, and invalid room types.

- [ ] **SQLite permission evaluator**
  - [ ] Load roles and role powers for a `UserUid` from SQLite.
  - [ ] Grant `e2ee_text` room visibility and read/write access through explicit room-to-role assignments rather than power thresholds alone.
  - [ ] Resolve the highest power per action and compare it to room requirements.
  - [ ] Replace the runtime `AllowAllPermissions` implementation.
  - [ ] Add tests for allowed and denied `join`, `write`, and `signal` requests.

- [ ] **Room discovery contract**
  - [ ] Add shared client/server messages for listing visible RAM voice rooms.
  - [ ] Return room ID, display name, and presence count without RAM chat history.
  - [ ] Build a client room list and replace manual UUID entry for normal use.
  - [ ] Keep direct UUID joining only as an explicitly marked development option, if needed.

- [ ] **Configuration and operational baseline**
  - [x] Load bind address, database path, and server name from configuration or environment variables.
  - [x] Preserve loopback-only binding as the default development setting.
  - [ ] Document LAN testing and the later requirements for WSS, STUN, and TURN.

## Out of Scope

- Persistent E2EE message storage, group key management, attachments, and media encryption.
- E2EE rich content delivery for encrypted images, files, links, and metadata.
- Voice-room video, screen sharing, SFU, and TURN deployment.
- Public Internet exposure and TLS certificate automation.
- A complete administrator UI for rooms, roles, and powers.