# Specification: WebRTC Voice Room Signaling Protocol

## 1. Overview
Voice communication in this platform occurs within designated **Voice Rooms**. 
Instead of a "calling/ringing" mechanism, users instantly connect to the room's audio stream upon entering the channel. 

For the **Minimum Viable Product (MVP)**, a **Full-Mesh WebRTC P2P** architecture is utilized. The self-hosted Rust server acts purely as a stateless **Signaling Broker** via WebSockets, transferring connection offers, answers, and network candidates between peers. The server itself never touches, processes, or logs the actual audio/video data packets.

---

## 2. Full-Mesh Architecture vs. Signaling

In a Full-Mesh architecture, every client inside the Voice Room establishes a direct, encrypted WebRTC peer connection with *every other client* in that room.

[Client A] <═══════(Direct WebRTC Audio)═══════> [Client B]
▲▲
║║
(Direct WebRTC)(Direct WebRTC)
║║
▼▼
<══════════════(Direct WebRTC Audio)═════════════> [Client C]

### The Role of the Server (Signaling Broker)
Before the direct P2P connections can be established, the clients must exchange their network capabilities (ICE Candidates) and session configurations (SDP Offers/Answers). This exchange is called **Signaling** and is routed in real-time through the server's WebSockets.

---

## 3. Signaling Message Protocol (JSON over WebSockets)

All signaling messages follow a uniform JSON structure wrapper over the WebSocket connection to allow the Rust backend to route them instantly without deep inspection.

### Base Wrapper Structure
```json
{
  "event": "voice_signaling",
  "room_id": "string_uuid",
  "target_uid": "recipient_public_key_hex",
  "payload": { ... }
}
```

### The 3 Core Signaling Stages

#### Phase A: The Join Broadcast
When Client A enters a Voice Room, the server registers the presence change and broadcasts a notification to all clients already inside that room.
* **Server to Existing Clients:** `{"event": "peer_joined", "peer_uid": "client_a_public_key"}`
* *Action:* Upon receiving this, the existing clients know they must now initiate a WebRTC handshake with the newcomer.

#### Phase B: SDP Offer & Answer Exchange
The existing clients (e.g., Client B) generate a WebRTC "Offer" (SDP string) and send it via the server to Client A.
1. **Client B to Server:** Sends event `signaling_offer` targeted to Client A.
2. **Server to Client A:** Forwards the offer.
3. **Client A to Server:** Processes the offer, generates a WebRTC "Answer" (SDP string), and sends event `signaling_answer` targeted to Client B.
4. **Server to Client B:** Forwards the answer.

#### Phase C: ICE Candidate Trickling
To bypass local firewalls and NAT routers, both clients gather network paths (ICE Candidates) via standard STUN servers and send them to each other continuously ("Trickling").
* **Client to Target via Server:** `{"event": "ice_candidate", "candidate": { ... }}`

---

## 4. State Machine & Lifecycle Flow

Client A (Joining)                Rust Server                 Client B (In Room)
│                                  │                                  │
├─► Join Room ────────────────────►│                                  │
│                                  ├─► Broadcast Presence ───────────►│
│                                  │                                  │
│                                  │◄─ Send SDP Offer (Target: A) ────┤
│◄─ Forward SDP Offer ─────────────┤                                  │
│                                  │                                  │
├─► Send SDP Answer (Target: B) ──►│                                  │
│                                  ├─► Forward SDP Answer ───────────►│
│                                  │                                  │
◄═══════════════════ Direct WebRTC P2P Audio ═════════════════════════►

### Disconnection Handling
When a user leaves the voice room or closes their app:
1. The WebSocket connection drops.
2. The Rust server detects the closed socket and instantly broadcasts a `{"event": "peer_left", "peer_uid": "..."}` to the remaining room members.
3. The remaining clients immediately tear down the local WebRTC hardware tracks and connections associated with that specific peer to free up system memory.

---

## 5. Security & Privacy Safeguards

* **Zero Audio Logs:** Because the WebRTC traffic flows directly between clients (P2P), it is technologically impossible for the server to log or intercept voice data.
* **Native WebRTC Encryption:** All WebRTC streams are mandatory encrypted using **DTLS-SRTP**. The encryption keys are negotiated directly between the clients during the handshake, meaning even a compromised signaling server cannot decrypt the voice traffic.
* **IP Privacy Note:** In a P2P mesh network, clients must know each other's IP addresses to connect. For true anonymity where IPs must be hidden from peers, the architecture will migrate to an **SFU model in Phase 2**, where clients only connect to the server's IP, hiding their local network addresses from other users.