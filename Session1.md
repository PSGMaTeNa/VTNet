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

- [ ] **Server-Domainlogik ohne Netzwerk implementieren**
  - [ ] `Session` mit verifizierter Identitaet, Anzeigename und optionaler aktiver `ram_voice`-Raum-ID modellieren.
  - [ ] `RoomRegistry` zur Verwaltung der aktiven Raumbelegung modellieren.
  - [ ] Join, automatischen Raumwechsel und `LeaveVoiceRoom` idempotent implementieren.
  - [ ] Serverseitig Senderidentitaet ergaenzen und Raum-/Berechtigungsmitgliedschaft fuer RAM-Text und WebRTC-Signaling pruefen.
  - [ ] Tests fuer Join, Wechsel A nach B, Leave, doppeltes Leave und verspaetetes Leave fuer A nach Wechsel zu B schreiben.

- [ ] **WebSocket-Transport anbinden**
  - [ ] Client-JSON in `ClientMessage` deserialisieren und Events aus der Domainlogik als `ServerMessage` versenden.
  - [ ] Unzulaessige oder ungueltige Nachrichten kontrolliert behandeln, ohne sensible Details zu senden oder Inhalte zu loggen.

- [ ] **Authentifizierung integrieren**
  - [ ] Verbindungszustaende `Connected`, `AuthChallengeSent` und `Authenticated` modellieren.
  - [ ] Nonce pro Verbindung erzeugen, einmalig akzeptieren und nach Ablauf bzw. Verwendung verwerfen.
  - [ ] Vor erfolgreicher Authentifizierung keine Raum-, Text- oder Signaling-Nachrichten verarbeiten.

- [ ] **Client-Praesenz und WebRTC**
  - [ ] Verbindung und aktive Voice-Raum-Praesenz in der Tauri/React-Oberflaeche darstellen.
  - [ ] `PeerJoinedRoom` fuer Offer-Erstellung sowie SDP- und ICE-Routing verwenden.
  - [ ] RAM-Text nur im aktiven Voice-Raum im React-State halten und beim Verlassen, Wechsel oder Verbindungsabbruch loeschen.

## Spaetere, getrennte Epics

- Persistente E2EE-Gruppenraeume: Schluesselmanagement, Mitgliederwechsel, authentifizierte Verschluesselung und Nachrichtenablage.
- Anhaenge und verschluesselte Medien.
- SFU, Gruppen-Video und Bildschirmfreigabe.
- Tatsaechtliche Dezentralisierung per Foederation, DHT oder P2P-Mesh.