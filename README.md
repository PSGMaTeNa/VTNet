# VTNet - Decentralized Hybrid Real-Time Communication

A decentralized, hybrid real-time communication platform (Text, Voice, Video) combining a modern usability and polished design combined with the sovereign security, privacy, and self-hosting principles like **TeamSpeak**.

---

## Key Features

* **Zero-Logs Architecture:** No logs for voice, video, or short-term ephemeral chats.
* **Sovereign Identity:** No central accounts. Identity is fully established through local asymmetric client-side cryptographic keypairs generated upon initial setup.(Accounts migth come later on voluntarily)
* **RAM-Only Rooms:** Ephemeral text channels that exist purely within the server's volatile memory (RAM). Once the server reboots or specific triggers happen, these messages vanish without a trace.
* **Post-Quantum E2EE Rooms:** Persistent chat rooms protected by cutting-edge **ML-KEM (Kyber)** End-to-End Encryption.
* **Self-Hosted & Compliance-Safe:** Complete digital sovereignty through fully decentralized self-hosting for communities and enterprises.

---

## Tech Stack

### Client / Frontend
* **TypeScript & React.js:** For a modern, high-performance, and visually polished "Discord-like" user interface.
* **Tauri Framework:** A lightweight, secure desktop application wrapper. Unlike memory-heavy Electron, Tauri utilizes the system's native webview, ensuring minimal RAM consumption.

### Network & Transport
* **WebSockets:** Used for real-time chat message delivery, system heartbeats, and signaling event routing.
* **WebRTC:** Powers high-quality, low-latency audio, video, and screen-sharing capabilities.

### Server / Backend
* **Rust:** Built for maximum memory safety, high-throughput packet processing, and extreme performance.
* **SQLite:** A lightweight, maintenance-free embedded database designed perfectly for self-hosted instances.

---

## Roadmap & Development Phases

### Phase 1: Minimum Viable Product (MVP)
* Local cryptographic keypair generation and secure identity storage.
* WebSocket connection setup to a single self-hosted Rust server instance.
* Volatile **RAM-Only text rooms** running on the server backend.
* Basic 1-on-1 voice calls using server-assisted WebRTC signaling(not final).

### Phase 2: Core Expansion
* Multicast voice channels using an intelligent **SFU (Selective Forwarding Unit)** architecture built into the Rust backend.
* Implementation of **ML-KEM (Kyber)** E2EE for persistent, encrypted text channels.
* Group video calls and screen-sharing integration.

### Phase 3: Total Decentralization
* Decentralized 1-on-1 direct messaging and calling bypassing any central server infrastructure via DHT (Distributed Hash Tables) or P2P networking mesh layers.
* Multi-device synchronization protocols for secure identity sharing.

---

## Contributing

We welcome contributions from the open-source community! To ensure legal safety and the project's long-term protection, all contributors must accept our **Contributor License Agreement (CLA)** before a Pull Request can be merged.

Please read our [CONTRIBUTING.md](CONTRIBUTING.md) and [CLA.md](CLA.md) for automated signing instructions.

---

## License

This project is licensed under the terms of the **GNU Affero General Public License v3.0** (AGPL-3.0) to ensure that the network core remains free and open for everyone. See the [LICENSE](LICENSE) file for the full text.
