# Session 2 - SQLite Room Management

## Goal

Replace the temporary in-memory assumptions with a persistent server structure. The server owns RAM voice-room definitions, user roles, and power requirements. Active voice presence and RAM text remain volatile and are never stored in SQLite.

## Order

- [ ] **SQLite foundation**
  - [ ] Add a SQLite dependency and a server-owned database configuration.
  - [ ] Create migrations for `server_config`, `trusted_users`, `roles`, `user_roles`, `rooms`, `room_permissions`, and `role_powers`.
  - [ ] Seed the single server configuration row and an initial administrator identity during server setup.
  - [ ] Add tests that verify schema creation and migration idempotency.

- [ ] **Persistent room definitions**
  - [ ] Model `ram_voice` and `e2ee_text` room types in a repository layer.
  - [ ] Require `JoinVoiceRoom` to target an existing `ram_voice` room.
  - [ ] Reject joins to unknown rooms and persistent E2EE rooms without changing active presence.
  - [ ] Add tests for valid rooms, missing rooms, and invalid room types.

- [ ] **SQLite permission evaluator**
  - [ ] Load roles and role powers for a `UserUid` from SQLite.
  - [ ] Resolve the highest power per action and compare it to room requirements.
  - [ ] Replace the runtime `AllowAllPermissions` implementation.
  - [ ] Add tests for allowed and denied `join`, `write`, and `signal` requests.

- [ ] **Room discovery contract**
  - [ ] Add shared client/server messages for listing visible RAM voice rooms.
  - [ ] Return room ID, display name, and presence count without RAM chat history.
  - [ ] Build a client room list and replace manual UUID entry for normal use.
  - [ ] Keep direct UUID joining only as an explicitly marked development option, if needed.

- [ ] **Configuration and operational baseline**
  - [ ] Load bind address, database path, and server name from configuration or environment variables.
  - [ ] Preserve loopback-only binding as the default development setting.
  - [ ] Document LAN testing and the later requirements for WSS, STUN, and TURN.

## Out of Scope

- Persistent E2EE message storage, group key management, attachments, and media encryption.
- SFU, group video, screen sharing, and TURN deployment.
- Public Internet exposure and TLS certificate automation.
- A complete administrator UI for rooms, roles, and powers.