# Session 1 - MVP Roadmap

## Zielbild

VTNet soll fuer selbst gehostete Communities einen TeamSpeak-aehnlichen `ram_voice`-Raum bieten: Ein Nutzer kann genau einen solchen Raum gleichzeitig belegen. Voice, WebRTC-Signaling und der gebundene RAM-Textchat sind fluechtig und werden nicht persistiert. Persistente E2EE-Textraeume sind davon getrennt; Nutzer koennen Mitglied mehrerer dieser Raeume sein.

## Reihenfolge

- [x] **Shared Wire Contract abschliessen**
  - [x] `UserUid` im JSON als Base64 statt als Array aus Zahlen serialisieren und beim Einlesen auf exakt 32 Bytes validieren.
  - [x] `RoomTextContent` als Nachrichtentyp einsetzen und seine Groessen- sowie Leertextvalidierung auch beim Deserialisieren erzwingen.
  - [x] `JoinVoiceRoom` verlaesst den vorherigen Voice-Raum automatisch; ein Join des aktuellen Raums ist idempotent und erzeugt keine doppelten Events.
  - [x] `room_id` in `PeerJoinedRoom` und `PeerLeftRoom` aufnehmen, damit Clients alte Events nach einem Raumwechsel verwerfen koennen.
  - [x] Das bytegenaue, versionsbehaftete Signaturformat fuer den Auth-Handshake festlegen.

- [x] **Server-Domainlogik ohne Netzwerk implementieren**
  - [x] `Session` mit verifizierter Identitaet, Anzeigename und optionaler aktiver `ram_voice`-Raum-ID modellieren.
  - [x] `RoomRegistry` zur Verwaltung der aktiven Raumbelegung modellieren.
  - [x] Join, automatischen Raumwechsel und `LeaveVoiceRoom` idempotent implementieren.
  - [x] Serverseitig Senderidentitaet ergaenzen und Raum-/Berechtigungsmitgliedschaft fuer RAM-Text und WebRTC-Signaling pruefen.
  - [x] Tests fuer Join, Wechsel A nach B, Leave, doppeltes Leave und verspaetetes Leave fuer A nach Wechsel zu B schreiben.

- [x] **WebSocket-Transport anbinden**
  - [x] Client-JSON in `ClientMessage` deserialisieren und Events aus der Domainlogik als `ServerMessage` versenden.
  - [x] Unzulaessige oder ungueltige Nachrichten kontrolliert behandeln, ohne sensible Details zu senden oder Inhalte zu loggen.

- [ ] **Permission-Modell vorbereiten**
  - [x] Den in `docs/05_permission_model.md` beschriebenen Permission-Evaluator als Server-Abstraktion modellieren, ohne SQLite oder eine Administrationsoberflaeche vorwegzunehmen.

- [x] **Authentifizierung integrieren**
  - [x] Verbindungszustaende `Connected`, `AuthChallengeSent` und `Authenticated` modellieren.
  - [x] Nonce pro Verbindung erzeugen, einmalig akzeptieren und nach Ablauf bzw. Verwendung verwerfen.
  - [x] Vor erfolgreicher Authentifizierung keine Raum-, Text- oder Signaling-Nachrichten verarbeiten.

- [x] **Client-Praesenz und WebRTC**
  - [x] Verbindung und aktive Voice-Raum-Praesenz in der Tauri/React-Oberflaeche darstellen.
  - [x] `PeerJoinedRoom` fuer Offer-Erstellung sowie SDP- und ICE-Routing verwenden.
  - [x] RAM-Text nur im aktiven Voice-Raum im React-State halten und beim Verlassen, Wechsel oder Verbindungsabbruch loeschen.

## Spaetere, getrennte Epics

- Persistente E2EE-Gruppenraeume: Schluesselmanagement, Mitgliederwechsel, authentifizierte Verschluesselung und Nachrichtenablage.
- Anhaenge und verschluesselte Medien.
- SFU, Gruppen-Video und Bildschirmfreigabe.
- Tatsaechtliche Dezentralisierung per Foederation, DHT oder P2P-Mesh.