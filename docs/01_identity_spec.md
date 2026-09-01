# Specification: Cryptographic Identity & Handshake Protocol

## 1. Overview
To ensure complete user sovereignty and eliminate the need for centralized registration or accounts, identity within this platform is purely cryptographic. 
Each user instance generates an asymmetric cryptographic keypair locally upon its first installation. This keypair acts as the globally unique identifier (UID).

---

## 2. Cryptographic Primitive Selection

For the **Minimum Viable Product (MVP)**, we separate the cryptographic identity (signing/authentication) from the future post-quantum transport layer.

* **Identity & Authentication:** **Ed25519** (Edwards-curve Digital Signature Algorithm).
  * *Rationale:* Extremely fast, highly secure, short keys (32-byte public key), and natively supported by modern Rust (`ed25519-dalek`) and TypeScript/WASM libraries.
  * **User ID (UID):** The hex or Base58-encoded representation of the Ed25519 Public Key serves as the user's visible address/identifier.
* **Future Upgrade Path (Phase 2):** **ML-KEM (Kyber)** will be layered on top for Ephemeral End-to-End Encryption key exchanges during room sessions. The core identity remains anchored to the Ed25519 signing key.

---

## 3. Client-Side Key Generation & Storage

When the Tauri client is launched for the absolute first time, it checks for the existence of a local identity file. If none is found, the generation sequence is triggered.

### Generation Flow (Tauri Core)
1. The **Tauri frontend (React)** detects a missing identity state and invokes a secure Tauri Command (`generate_identity`).
2. The **Tauri backend (Rust)** utilizes a cryptographically secure pseudo-random number generator (CSPRNG) via the `rand` crate.
3. An Ed25519 keypair is generated.

### Secure Local Storage
To prevent unauthorized extraction of the private key by other local software, store it in the OS credential store; the Tauri app data directory should only contain non-secret metadata (public key + keyring entry identifier).
* **Linux:** Stored via `libsecret` / GNOME Keyring.
* **macOS:** Stored via the native OS Keychain.
* **Windows:** Stored via the Credential Manager (DPAPI).
* *Implementation details:* Use a Rust keyring abstraction (e.g., the `keyring` crate) to interface with these stores automatically.

---

## 4. Proof-of-Possession (Challenge–Response) Handshake Protocol

When a client connects to a self-hosted server via WebSockets, the server must verify that the client actually owns the Private Key corresponding to their claimed Public Key—**without the client ever revealing the Private Key.**
(This is a challenge–response proof-of-possession check rather than a general “zero-knowledge proof” protocol.)
Client                                                     Server
1. Connect (WS Request + Public Key)
├─────────────────────────────────────────────────────────►│
                                                            2. Challenge (Cryptographic Nonce + Timestamp) 
◄─────────────────────────────────────────────────────────┤
3. Response (Signature of Nonce + Timestamp)
├─────────────────────────────────────────────────────────►|
                                                            4. Verification & Session Established
│◄─────────────────────────────────────────────────────────┤

### Step-by-Step Flow:
1. **Initiation:** Client opens a WebSocket connection to the server and transmits its `Public Key` as the initial greeting payload.
2. **Challenge:** The server generates a unique, cryptographically secure 32-byte (`Nonce`) combined with a current high-precision `Timestamp`. The server stores this temporary challenge, the client's public key, and the negotiated protocol version in volatile memory tied to the WebSocket connection.
3. **Signing:** The Tauri client receives the challenge, sends it to the Tauri Rust core, and signs the following canonical binary sequence using its locally stored `Private Key`:

  ```text
  "VTNET_AUTH_v1" | protocol_version as u16 big-endian | nonce as 32 raw bytes |
  server_timestamp as u64 big-endian | client_public_key as 32 raw bytes
  ```

  The client transmits the resulting 64-byte Ed25519 signature as Base64. The shared `auth_signature_payload` function is the normative implementation of this sequence.
4. **Verification:** The server utilizes the client's public key to verify the signature against the exact challenge it issued.
  * If the signature is valid and the temporary server-side challenge has not expired (e.g., after 5 seconds), the connection is promoted to an **Authenticated Session**.
   * If it fails, the WebSocket connection is instantly terminated with a `4401 Unauthorized` frame.

---

## 5. Security & Threat Modeling

* **Replay Attacks:** Mitigated by a unique, short-lived, server-side nonce which is consumed after a verification attempt. An intercepted signature cannot be reused because it is bound to one issued challenge, one public key, and one protocol version.
* **Man-in-the-Middle (MitM):** While the handshake proves identity, the entire WebSocket transport layer must be wrapped in standard **TLS (WSS)** to prevent session hijacking.
* **Key Loss:** Since there are no central servers, losing the local private key means losing access to that identity forever. For the MVP, this is accepted behavior (true sovereignty). Phase 3 migth introduce an encrypted backup/seed-phrase mechanism.