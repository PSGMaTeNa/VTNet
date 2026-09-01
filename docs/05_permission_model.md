# Specification: Role and Power Permission Model

## 1. Goal

VTNet uses a TeamSpeak-inspired permission model while keeping normal administration understandable. Administrators assign named roles to users and configure the permissions provided by each role. Rooms define the power required for individual actions.

## 2. Roles

A user may hold one or more server roles, such as `guest`, `member`, `moderator`, or `admin`. Roles are server-local metadata and do not change the user's cryptographic `UserUid`.

For the MVP, roles are the primary administration interface. Room access can be configured through the powers granted by these roles, without exposing the full power model in the client UI.

## 3. Powers

Each role can grant a numeric power for an action. A room defines the minimum power required to perform that action.

```text
effective_power(action) = max(power(role_1, action), power(role_2, action), ...)
access is granted when effective_power(action) >= required_power(room, action)
```

The initial action set is:

- `join`: Enter a RAM voice room.
- `speak`: Send voice media in a RAM voice room.
- `write`: Send ephemeral RAM-room text.
- `signal`: Route WebRTC SDP and ICE messages.
- `manage_room`: Change room configuration or permissions.

`signal` should normally require the same or lower power than `speak`; signaling is necessary to establish a voice connection but is not a permission to transmit media.

## 4. Room Access

Before a session enters a RAM voice room, the server evaluates `join`. The server evaluates `write` before routing RAM text and `signal` before forwarding WebRTC SDP or ICE. Voice-media authorization is enforced by the client and later by the selected media architecture, but the server remains the authority for room admission.

Persistent E2EE text rooms use the same role and power source, but their `read` and `write` rules are independent of active RAM voice-room presence.

## 5. Future Extensions

The initial model has no explicit deny rules. The highest granted power wins for each action, which keeps decisions predictable. Later versions may add room-specific role overrides and user-specific overrides, provided the final evaluator remains explainable to administrators.

The server domain must depend on a permission-evaluation interface rather than direct database queries. SQLite storage, role management, and an administration UI are separate implementation steps.