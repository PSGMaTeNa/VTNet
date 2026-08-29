# Specification: Server-Side Volatile RAM-Only Rooms (Real-Time Only)

## 1. Overview
To achieve the absolute highest standard of ephemeral privacy, certain channels are designated as **RAM-Only Rooms**. 
These rooms operate on a strict **Real-Time-Only** principle: The server acts exclusively as a stateless message router (broker). Messages exist in the server's volatile memory (RAM) only for the fraction of a millisecond required to parse and forward them to active listeners. 

There is **no message history, no buffer, and no caching**. Once a message is routed, it is gone forever.

---

## 2. Stateless Routing Architecture (Rust Backend)

Since no conversation history is retained on the server, the architecture changes from a storage model to a pure pub/sub (Publish/Subscribe) routing model.

* **Routing Mechanism:** When a text message is received via an authenticated WebSocket connection, the server immediately identifies all other connected clients currently present in that specific room.
* **Concurrency:** The server iterates through the active WebSocket write-handles using efficient, asynchronous task scheduling (`tokio`).
* **Immediate Purge:** The memory allocated for the message payload is dropped automatically by Rust's ownership system as soon as the broadcast loop finishes.

---

## 3. Strict No-History Protocol (The Radio Principle)

When a client joins a RAM-Only room or reconnects after being disconnected, they will face a completely empty interface.

* **No History Fetch:** The server provides no synchronization payload upon room entry.
* **Client-Side Volatility:** The Tauri frontend (React) is strictly prohibited from storing or caching these messages locally (e.g., in `localStorage` or IndexedDB). Messages are kept only in the active React state. 
* **Session Cleardown:** As soon as the user switches to a different room, closes the app, or loses connection, the local UI state is instantly wiped.

---

## 4. Message Lifecycle & Routing Flow


[Client A] ──(WS: New Message)──► [Rust Server]│
                                               ├─► 1. Identify connected peers in room
                                               ├─► 2. Instantly forward bytes to active sockets
                                               ├─► 3. Drop/Free message memory immediately
       │[Client B (In Room)] ◄──(WS: Broadcast)┴─► [Client C (In Room)]

[Client D (Offline/Other Room)] ────────► (Receives Nothing / Zero Trace)


### 1. Ingestion
* Client A transmits a message containing the target room ID and the payload.

### 2. Broadcast
* The server distributes the payload to Client B and Client C in real-time.

### 3. Immediate Eviction (Zeroize)
* If the message contained highly sensitive transient data, the backend explicitly ensures that the text memory buffers are wiped using the `Zeroize` crate trait before the memory page is returned to the OS allocator.

---

## 5. Architectural Safeguards (DoS Prevention)

To guarantee stability for self-hosted instances, strict limits are enforced per room:

| Parameter                 | Limit Value           | Purpose                                           |
| :---                      | :---                  | :---                                              |
| **Max Payload Size**      | 4 KB per Message      | Limits memory per text injection                  |
| **Rate Limiting**         | 5 Messages / 2 Seconds| Protects against spam bots filling the buffer     |
| **File Transfers**        | Forbidden             | RAM rooms do not store binary data (Images/Files) |
| **Audit Trails**          | Strictly Disabled     | Server logs will never contain message data       |
