# Specification: Permanent Server-Side Database Schema (SQLite)

## 1. Overview
The self-hosted Rust backend utilizes **SQLite** for metadata, structural persistence, and encrypted message storage. 

To honor the privacy architecture, this database operates under a strict policy for user content:
1. **Server Structure & Settings (Plaintext):** Server settings, room names, user roles, and access rights are stored in plaintext so the Rust backend can manage permissions and route network connections.
2. **RAM-Only Voice Rooms (Transient):** A voice room and its bound ephemeral text chat share one room identity. Their text communication is completely excluded from database persistence.
3. **Persistent E2EE Rooms (Encrypted):** Message history and rich media (images/files) are stored permanently, but they are strictly encrypted via End-to-End Encryption (ML-KEM/Kyber) on the client side before transmission. The server only stores unreadable cryptographic ciphertexts and binary blobs.

---

## 2. Table Schemas & DDL (Data Definition Language)

### 1. `server_config`
Stores core global configurations for the self-hosted instance. This table will always contain exactly one row.

```sql
CREATE TABLE server_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    server_name TEXT NOT NULL DEFAULT 'Sovereign Hub',
    motd TEXT,                                      -- Message of the Day
    allow_public_registration INTEGER NOT NULL DEFAULT 1, -- 0 = Invite Only, 1 = Open
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### 2. `trusted_users`
Maps cryptographic public keys (Client-Keys) to server-specific roles (e.g., Administrator, Moderator, User).

```sql
CREATE TABLE trusted_users (
    user_uid TEXT PRIMARY KEY,                       -- Hex/Base58 encoded Ed25519 Public Key
    display_name TEXT NOT NULL,                     -- Custom nickname chosen for this server
    server_role TEXT NOT NULL DEFAULT 'user',        -- 'admin', 'moderator', 'user', 'guest'
    is_banned INTEGER NOT NULL DEFAULT 0,           -- 1 = Banned from server
    joined_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX idx_user_uid ON trusted_users(user_uid);
```

### 3. `rooms`
Defines the layout of the server. A `ram_voice` room combines a live voice channel with its bound ephemeral text chat; a user may occupy only one such room at a time. Persistent E2EE text rooms are independent of voice-room presence.

```sql
CREATE TABLE rooms (
    room_id TEXT PRIMARY KEY,                       -- UUID v4
    name TEXT NOT NULL,                             -- Visible room name
    room_type TEXT NOT NULL,                        -- 'ram_voice', 'e2ee_text'
    sort_order INTEGER NOT NULL DEFAULT 0,          -- Layout positioning in UI
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### 4. `room_permissions`
Manages access control on a per-room basis for cryptographic identities.

```sql
CREATE TABLE room_permissions (
    room_id TEXT NOT NULL,
    user_uid TEXT NOT NULL,
    can_read INTEGER NOT NULL DEFAULT 1,            -- Applicable to E2EE rooms and RAM voice rooms
    can_write INTEGER NOT NULL DEFAULT 1,           -- Allows E2EE messages or ephemeral RAM-room text
    can_speak INTEGER NOT NULL DEFAULT 1,           -- Applicable to RAM voice rooms
    PRIMARY KEY (room_id, user_uid),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_uid) REFERENCES trusted_users(user_uid) ON DELETE CASCADE
);
```

### 5. `encrypted_messages`
Stores the permanent history for persistent E2EE rooms. Every message payload is an encrypted string or blob that can only be decrypted by keys held by authorized clients.

```sql
CREATE TABLE encrypted_messages (
    message_id TEXT PRIMARY KEY,                     -- UUID v4
    room_id TEXT NOT NULL,                           -- Foreign key referencing rooms
    sender_uid TEXT NOT NULL,                        -- Public key of the sender
    encrypted_payload TEXT NOT NULL,                -- Base64 encoded ciphertext (E2EE Text/Metadata)
    nonce TEXT NOT NULL,                            -- Unique initialization vector for decryption
    server_timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (sender_uid) REFERENCES trusted_users(user_uid)
);
CREATE INDEX idx_room_messages ON encrypted_messages(room_id);
```

### 6. `encrypted_attachments`
Handles rich media files (images, documents) uploaded to persistent E2EE rooms. Files are encrypted client-side and stored either as binary blobs (for small sizes) or referenced as local encrypted files managed by the Rust storage layer.

```sql
CREATE TABLE encrypted_attachments (
    attachment_id TEXT PRIMARY KEY,                 -- UUID v4
    message_id TEXT NOT NULL,                       -- Associated message
    encrypted_filename TEXT NOT NULL,               -- Encrypted name string
    file_size_bytes INTEGER NOT NULL,
    encrypted_blob BLOB,                            -- Optional: Direct storage for small thumbnails/images
    local_file_path TEXT,                           -- Optional: Path on server disk for larger files
    FOREIGN KEY (message_id) REFERENCES encrypted_messages(message_id) ON DELETE CASCADE
);
```

---

## 3. Privacy & Compliance Matrix

|Data Type                 | Storage Location  | Server Visibility      | Retention Period                         |   
|:---                      | :---              | :---                   | :---                                     |
|**RAM Voice Room Text**   | Volatile Memory   | Plaintext(In-flight)   | 0 milliseconds (Dropped after broadcast) |
|**RAM Voice Room Media**  | None (P2P Mesh)   | None (Encrypted P2P)   | 0 milliseconds                           |
|**E2EE Room History**     | SQLite Database   | **Ciphertext Only**    | Permanent (Until deleted by user/admin)  |
|**Media Attachments**     | SQLite/Server Disk| **Encrypted Blobs**    | Permanent (Until deleted by user/admin)  |
|**Server & Room Settings**| SQLite Database   | Plaintext Configuration| Permanent (Required for operation)       |
|**User Identifiers**      | SQLite Database   | Plaintext Public Keys  | Permanent (Required for access control)  |
