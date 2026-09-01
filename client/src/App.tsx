import { FormEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

const SERVER_URL = "ws://127.0.0.1:3000/ws";
const PROTOCOL_VERSION = 1;

type ConnectionStatus = "disconnected" | "connecting" | "authenticating" | "connected";
type IdentityInfo = { user_uid: string };
type ServerEnvelope = { event: string; payload: Record<string, unknown> };
type ChatMessage = { senderName: string; text: string; timestamp: number };

function App() {
  const socketRef = useRef<WebSocket | null>(null);
  const activeRoomRef = useRef<string | null>(null);
  const localStreamRef = useRef<MediaStream | null>(null);
  const peerConnectionsRef = useRef(new Map<string, RTCPeerConnection>());
  const remoteAudioRef = useRef(new Map<string, HTMLAudioElement>());
  const [identity, setIdentity] = useState<IdentityInfo | null>(null);
  const [connectionStatus, setConnectionStatus] = useState<ConnectionStatus>("disconnected");
  const [activeRoomId, setActiveRoomId] = useState<string | null>(null);
  const [roomIdInput, setRoomIdInput] = useState("");
  const [textInput, setTextInput] = useState("");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isMuted, setIsMuted] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  useEffect(() => {
    void invoke<IdentityInfo>("get_or_create_identity")
      .then(setIdentity)
      .catch(() => setErrorMessage("Could not load the local identity."));
  }, []);

  useEffect(() => {
    activeRoomRef.current = activeRoomId;
  }, [activeRoomId]);

  useEffect(() => () => {
    socketRef.current?.close();
    cleanupMedia();
  }, []);

  function sendMessage(event: string, payload: Record<string, unknown>) {
    if (socketRef.current?.readyState === WebSocket.OPEN) {
      socketRef.current.send(JSON.stringify({ event, payload }));
      return true;
    }

    return false;
  }

  function cleanupMedia() {
    peerConnectionsRef.current.forEach((connection) => connection.close());
    peerConnectionsRef.current.clear();
    remoteAudioRef.current.forEach((audio) => audio.remove());
    remoteAudioRef.current.clear();
    localStreamRef.current?.getTracks().forEach((track) => track.stop());
    localStreamRef.current = null;
    setIsMuted(false);
  }

  async function prepareAudio() {
    if (localStreamRef.current) return true;
    try {
      localStreamRef.current = await navigator.mediaDevices.getUserMedia({ audio: true });
      localStreamRef.current.getAudioTracks().forEach((track) => track.enabled = !isMuted);
      return true;
    } catch {
      setErrorMessage("Microphone access is required to join a voice room.");
      return false;
    }
  }

  function peerConnection(peerUid: string) {
    const existing = peerConnectionsRef.current.get(peerUid);
    if (existing) return existing;

    const connection = new RTCPeerConnection();
    localStreamRef.current?.getTracks().forEach((track) => connection.addTrack(track, localStreamRef.current!));
    connection.addEventListener("icecandidate", (event) => {
      if (event.candidate && activeRoomRef.current) {
        sendMessage("send_web_rtc_ice", {
          room_id: activeRoomRef.current,
          target_uid: peerUid,
          candidate: event.candidate.toJSON(),
        });
      }
    });
    connection.addEventListener("track", (event) => {
      const audio = new Audio();
      audio.autoplay = true;
      audio.srcObject = event.streams[0];
      remoteAudioRef.current.set(peerUid, audio);
      void audio.play().catch(() => undefined);
    });
    peerConnectionsRef.current.set(peerUid, connection);
    return connection;
  }

  function removePeer(peerUid: string) {
    peerConnectionsRef.current.get(peerUid)?.close();
    peerConnectionsRef.current.delete(peerUid);
    remoteAudioRef.current.get(peerUid)?.remove();
    remoteAudioRef.current.delete(peerUid);
  }

  function toggleMute() {
    const nextMuted = !isMuted;
    localStreamRef.current?.getAudioTracks().forEach((track) => track.enabled = !nextMuted);
    setIsMuted(nextMuted);
  }

  async function handleServerMessage(rawPayload: string) {
    let message: ServerEnvelope;
    try {
      message = JSON.parse(rawPayload) as ServerEnvelope;
    } catch {
      setErrorMessage("The server sent an invalid protocol message.");
      return;
    }

    if (message.event === "auth_challenge") {
      const nonceBase64 = message.payload.nonce_base64;
      const serverTimestamp = message.payload.server_timestamp;
      if (typeof nonceBase64 !== "string" || typeof serverTimestamp !== "number") {
        setErrorMessage("The authentication challenge was incomplete.");
        return;
      }

      try {
        const signatureBase64 = await invoke<string>("sign_auth_challenge", {
          request: { protocol_version: PROTOCOL_VERSION, nonce_base64: nonceBase64, server_timestamp: serverTimestamp },
        });
        sendMessage("auth_response", { signature_base64: signatureBase64 });
      } catch {
        setErrorMessage("Could not sign the authentication challenge.");
      }
      return;
    }

    if (message.event === "auth_success") {
      setConnectionStatus("connected");
      setErrorMessage(null);
      return;
    }

    if (message.event === "auth_failure") {
      const reason = message.payload.reason;
      setErrorMessage(
        reason === "request_expired"
          ? "The authentication challenge expired. Please connect again."
          : "Authentication was rejected by the server.",
      );
      socketRef.current?.close();
      return;
    }

    if (message.event === "voice_room_joined" && typeof message.payload.room_id === "string") {
      peerConnectionsRef.current.forEach((connection) => connection.close());
      peerConnectionsRef.current.clear();
      remoteAudioRef.current.forEach((audio) => audio.remove());
      remoteAudioRef.current.clear();
      setActiveRoomId(message.payload.room_id);
      setMessages([]);
      return;
    }

    if (message.event === "voice_room_left") {
      setActiveRoomId(null);
      setMessages([]);
      cleanupMedia();
      return;
    }

    if (message.event === "peer_joined_room" && typeof message.payload.peer_uid === "string") {
      const peerUid = message.payload.peer_uid;
      const roomId = message.payload.room_id;
      if (roomId !== activeRoomRef.current || peerUid === identity?.user_uid) return;
      const connection = peerConnection(peerUid);
      const offer = await connection.createOffer();
      await connection.setLocalDescription(offer);
      sendMessage("send_web_rtc_sdp", { room_id: roomId, target_uid: peerUid, sdp_type: "offer", sdp_raw: offer.sdp ?? "" });
      return;
    }

    if (message.event === "peer_left_room" && typeof message.payload.peer_uid === "string" && message.payload.room_id === activeRoomRef.current) {
      removePeer(message.payload.peer_uid);
      return;
    }

    if (message.event === "forward_web_rtc_sdp" && typeof message.payload.sender_uid === "string" && typeof message.payload.sdp_type === "string" && typeof message.payload.sdp_raw === "string" && message.payload.room_id === activeRoomRef.current) {
      const peerUid = message.payload.sender_uid;
      const connection = peerConnection(peerUid);
      await connection.setRemoteDescription({ type: message.payload.sdp_type as RTCSdpType, sdp: message.payload.sdp_raw });
      if (message.payload.sdp_type === "offer") {
        const answer = await connection.createAnswer();
        await connection.setLocalDescription(answer);
        sendMessage("send_web_rtc_sdp", { room_id: activeRoomRef.current, target_uid: peerUid, sdp_type: "answer", sdp_raw: answer.sdp ?? "" });
      }
      return;
    }

    if (message.event === "forward_web_rtc_ice" && typeof message.payload.sender_uid === "string" && message.payload.room_id === activeRoomRef.current && message.payload.candidate && typeof message.payload.candidate === "object") {
      await peerConnection(message.payload.sender_uid).addIceCandidate(message.payload.candidate as RTCIceCandidateInit);
      return;
    }

    if (message.event === "broadcast_text_message" && message.payload.room_id === activeRoomRef.current && typeof message.payload.sender_name === "string" && typeof message.payload.text_content === "string" && typeof message.payload.server_timestamp === "number") {
      const senderName = message.payload.sender_name;
      const text = message.payload.text_content;
      const timestamp = message.payload.server_timestamp;
      setMessages((currentMessages) => [...currentMessages, { senderName, text, timestamp }]);
    }
  }

  async function connect() {
    if (connectionStatus !== "disconnected") return;
    setConnectionStatus("connecting");
    setErrorMessage(null);
    let currentIdentity: IdentityInfo;
    try {
      currentIdentity = await invoke<IdentityInfo>("get_or_create_identity");
      setIdentity(currentIdentity);
    } catch {
      setConnectionStatus("disconnected");
      setErrorMessage("Could not load the local identity.");
      return;
    }

    const socket = new WebSocket(SERVER_URL);
    socketRef.current = socket;
    socket.addEventListener("open", () => {
      setConnectionStatus("authenticating");
      sendMessage("auth_identify", { protocol_version: PROTOCOL_VERSION, client_uid: currentIdentity.user_uid });
    });
    socket.addEventListener("message", (event) => void handleServerMessage(event.data));
    socket.addEventListener("error", () => setErrorMessage("Could not connect to the local server."));
    socket.addEventListener("close", () => {
      socketRef.current = null;
      activeRoomRef.current = null;
      setConnectionStatus("disconnected");
      setActiveRoomId(null);
      setMessages([]);
      cleanupMedia();
    });
  }

  async function joinRoom(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!isUuid(roomIdInput)) {
      setErrorMessage("Enter a valid room UUID.");
      return;
    }
    if (!(await prepareAudio())) return;
    sendMessage("join_voice_room", { room_id: roomIdInput });
  }

  function sendText(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (activeRoomId && textInput.trim()) {
      const text = textInput;
      if (sendMessage("send_text_message", { room_id: activeRoomId, text_content: text })) {
        setMessages((currentMessages) => [...currentMessages, { senderName: "You", text, timestamp: Date.now() }]);
        setTextInput("");
      }
    }
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <div><p className="eyebrow">VTNET / LOCAL NODE</p><h1>Voice rooms</h1></div>
        <div className={`connection-state ${connectionStatus}`}><span aria-hidden="true" />{connectionStatus}</div>
      </header>
      <section className="workspace" aria-label="Voice room connection">
        <aside className="connection-panel">
          <p className="panel-label">Identity</p>
          <p className="identity-value">{identity?.user_uid ?? "Loading local key..."}</p>
          <div className="connection-actions">
            <button type="button" onClick={connect} disabled={!identity || connectionStatus !== "disconnected"}>Connect</button>
            <button type="button" className="secondary" onClick={() => socketRef.current?.close()} disabled={connectionStatus === "disconnected"}>Disconnect</button>
          </div>
          <form className="room-form" onSubmit={joinRoom}>
            <label htmlFor="room-id">Voice room UUID</label>
            <input id="room-id" value={roomIdInput} onChange={(event) => setRoomIdInput(event.currentTarget.value)} placeholder="xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" disabled={connectionStatus !== "connected"} />
            <button type="submit" disabled={connectionStatus !== "connected"}>Join room</button>
          </form>
        </aside>
        <section className="room-panel">
          <div className="room-heading">
            <div><p className="panel-label">Active RAM voice room</p><h2>{activeRoomId ?? "No active room"}</h2></div>
            <div className="voice-controls">
              <span className={`microphone-state ${isMuted ? "muted" : "live"}`}>{isMuted ? "Muted" : "Mic live"}</span>
              <button type="button" className="secondary" onClick={toggleMute} disabled={!activeRoomId}>{isMuted ? "Unmute" : "Mute"}</button>
              <button type="button" className="secondary" onClick={() => activeRoomId && sendMessage("leave_voice_room", { room_id: activeRoomId })} disabled={!activeRoomId}>Leave room</button>
            </div>
          </div>
          <div className="chat-log" aria-live="polite">
            {activeRoomId && messages.length === 0 && <p className="empty-state">No messages in this room yet.</p>}
            {!activeRoomId && <p className="empty-state">Join a voice room to use its ephemeral text channel.</p>}
            {messages.map((message) => <article className="chat-message" key={`${message.timestamp}-${message.senderName}-${message.text}`}><strong>{message.senderName}</strong><p>{message.text}</p></article>)}
          </div>
          <form className="chat-form" onSubmit={sendText}>
            <input value={textInput} onChange={(event) => setTextInput(event.currentTarget.value)} placeholder="Message the active room" maxLength={4096} disabled={!activeRoomId} />
            <button type="submit" disabled={!activeRoomId || !textInput.trim()}>Send</button>
          </form>
        </section>
      </section>
      {errorMessage && <p className="error-message" role="alert">{errorMessage}</p>}
    </main>
  );
}

function isUuid(value: string) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

export default App;
