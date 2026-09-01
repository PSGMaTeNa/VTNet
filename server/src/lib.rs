use std::collections::{HashMap, HashSet};

use shared::{ClientMessage, ServerMessage, UserUid};
use uuid::Uuid;

pub mod transport;
pub mod auth;

/// Authenticated connection state relevant to voice-room presence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    user_uid: UserUid,
    display_name: String,
    active_voice_room_id: Option<Uuid>,
}

impl Session {
    pub fn new(user_uid: UserUid, display_name: String) -> Self {
        Self {
            user_uid,
            display_name,
            active_voice_room_id: None,
        }
    }

    pub fn user_uid(&self) -> UserUid {
        self.user_uid
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn active_voice_room_id(&self) -> Option<Uuid> {
        self.active_voice_room_id
    }
}

/// Tracks active members in each ephemeral RAM voice room.
#[derive(Default)]
pub struct RoomRegistry {
    members_by_room: HashMap<Uuid, HashSet<UserUid>>,
}

/// Describes the state change caused by a voice-room request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceRoomTransition {
    NoChange,
    Joined { room_id: Uuid },
    Switched { from_room_id: Uuid, to_room_id: Uuid },
    Left { room_id: Uuid },
}

/// Actions that can require a separate power level in a room.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RoomAction {
    Join,
    Speak,
    Write,
    Signal,
    ManageRoom,
}

/// Resolves a user's power and the room power required for an action.
pub trait PermissionEvaluator {
    fn effective_power(&self, user_uid: UserUid, action: RoomAction) -> u16;

    fn required_power(&self, room_id: Uuid, action: RoomAction) -> u16;

    fn is_allowed(&self, user_uid: UserUid, room_id: Uuid, action: RoomAction) -> bool {
        self.effective_power(user_uid, action) >= self.required_power(room_id, action)
    }
}

/// Indicates why a session cannot enter a voice room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomJoinError {
    PermissionDenied,
}

/// A trusted server message and the sessions that must receive it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutedServerMessage {
    recipients: HashSet<UserUid>,
    message: ServerMessage,
}

impl RoutedServerMessage {
    pub fn recipients(&self) -> &HashSet<UserUid> {
        &self.recipients
    }

    pub fn message(&self) -> &ServerMessage {
        &self.message
    }
}

/// Indicates why a client message cannot be routed within a voice room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomRoutingError {
    PermissionDenied { action: RoomAction },
    SenderNotInActiveVoiceRoom,
    TargetNotInActiveVoiceRoom,
    UnsupportedClientMessage,
}

impl RoomRegistry {
    pub fn members(&self, room_id: Uuid) -> Option<&HashSet<UserUid>> {
        self.members_by_room.get(&room_id)
    }

    pub fn join_voice_room(
        &mut self,
        session: &mut Session,
        room_id: Uuid,
        permission_evaluator: &impl PermissionEvaluator,
    ) -> Result<VoiceRoomTransition, RoomJoinError> {
        if !permission_evaluator.is_allowed(session.user_uid, room_id, RoomAction::Join) {
            return Err(RoomJoinError::PermissionDenied);
        }

        match session.active_voice_room_id {
            Some(active_room_id) if active_room_id == room_id => Ok(VoiceRoomTransition::NoChange),
            Some(previous_room_id) => {
                // Remove the previous membership before admitting the session to the next room.
                self.remove_member(previous_room_id, session.user_uid);
                self.add_member(room_id, session.user_uid);
                session.active_voice_room_id = Some(room_id);

                Ok(VoiceRoomTransition::Switched {
                    from_room_id: previous_room_id,
                    to_room_id: room_id,
                })
            }
            None => {
                self.add_member(room_id, session.user_uid);
                session.active_voice_room_id = Some(room_id);

                Ok(VoiceRoomTransition::Joined { room_id })
            }
        }
    }

    pub fn leave_voice_room(
        &mut self,
        session: &mut Session,
        room_id: Uuid,
    ) -> VoiceRoomTransition {
        if session.active_voice_room_id != Some(room_id) {
            return VoiceRoomTransition::NoChange;
        }

        self.remove_member(room_id, session.user_uid);
        session.active_voice_room_id = None;

        VoiceRoomTransition::Left { room_id }
    }

    pub fn route_client_room_message(
        &self,
        session: &Session,
        message: ClientMessage,
        server_timestamp: u64,
        permission_evaluator: &impl PermissionEvaluator,
    ) -> Result<RoutedServerMessage, RoomRoutingError> {
        match message {
            ClientMessage::SendTextMessage {
                room_id,
                text_content,
            } => {
                self.ensure_active_member(session, room_id)?;
                self.ensure_permission(
                    permission_evaluator,
                    session.user_uid,
                    room_id,
                    RoomAction::Write,
                )?;

                Ok(RoutedServerMessage {
                    recipients: self.other_members(room_id, session.user_uid),
                    message: ServerMessage::BroadcastTextMessage {
                        room_id,
                        sender_uid: session.user_uid,
                        sender_name: session.display_name.clone(),
                        text_content,
                        server_timestamp,
                    },
                })
            }
            ClientMessage::SendWebRtcSdp {
                room_id,
                target_uid,
                sdp_type,
                sdp_raw,
            } => {
                self.ensure_active_member(session, room_id)?;
                self.ensure_permission(
                    permission_evaluator,
                    session.user_uid,
                    room_id,
                    RoomAction::Signal,
                )?;
                self.ensure_target_member(room_id, target_uid)?;

                Ok(RoutedServerMessage {
                    recipients: HashSet::from([target_uid]),
                    message: ServerMessage::ForwardWebRtcSdp {
                        room_id,
                        sender_uid: session.user_uid,
                        sdp_type,
                        sdp_raw,
                    },
                })
            }
            ClientMessage::SendWebRtcIce {
                room_id,
                target_uid,
                candidate,
            } => {
                self.ensure_active_member(session, room_id)?;
                self.ensure_permission(
                    permission_evaluator,
                    session.user_uid,
                    room_id,
                    RoomAction::Signal,
                )?;
                self.ensure_target_member(room_id, target_uid)?;

                Ok(RoutedServerMessage {
                    recipients: HashSet::from([target_uid]),
                    message: ServerMessage::ForwardWebRtcIce {
                        room_id,
                        sender_uid: session.user_uid,
                        candidate,
                    },
                })
            }
            _ => Err(RoomRoutingError::UnsupportedClientMessage),
        }
    }

    fn ensure_active_member(
        &self,
        session: &Session,
        room_id: Uuid,
    ) -> Result<(), RoomRoutingError> {
        if session.active_voice_room_id == Some(room_id)
            && self
                .members(room_id)
                .is_some_and(|members| members.contains(&session.user_uid))
        {
            Ok(())
        } else {
            Err(RoomRoutingError::SenderNotInActiveVoiceRoom)
        }
    }

    fn ensure_target_member(
        &self,
        room_id: Uuid,
        target_uid: UserUid,
    ) -> Result<(), RoomRoutingError> {
        if self
            .members(room_id)
            .is_some_and(|members| members.contains(&target_uid))
        {
            Ok(())
        } else {
            Err(RoomRoutingError::TargetNotInActiveVoiceRoom)
        }
    }

    fn ensure_permission(
        &self,
        permission_evaluator: &impl PermissionEvaluator,
        user_uid: UserUid,
        room_id: Uuid,
        action: RoomAction,
    ) -> Result<(), RoomRoutingError> {
        if permission_evaluator.is_allowed(user_uid, room_id, action) {
            Ok(())
        } else {
            Err(RoomRoutingError::PermissionDenied { action })
        }
    }

    fn other_members(&self, room_id: Uuid, user_uid: UserUid) -> HashSet<UserUid> {
        self.members(room_id)
            .into_iter()
            .flat_map(|members| members.iter().copied())
            .filter(|member_uid| *member_uid != user_uid)
            .collect()
    }

    fn add_member(&mut self, room_id: Uuid, user_uid: UserUid) {
        self.members_by_room
            .entry(room_id)
            .or_default()
            .insert(user_uid);
    }

    fn remove_member(&mut self, room_id: Uuid, user_uid: UserUid) {
        // Empty rooms have no registry entry, making a missing room equivalent to no members.
        let should_remove_room = self
            .members_by_room
            .get_mut(&room_id)
            .is_some_and(|members| {
                members.remove(&user_uid);
                members.is_empty()
            });

        if should_remove_room {
            self.members_by_room.remove(&room_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AllowAllPermissions;

    impl PermissionEvaluator for AllowAllPermissions {
        fn effective_power(&self, _: UserUid, _: RoomAction) -> u16 {
            1
        }

        fn required_power(&self, _: Uuid, _: RoomAction) -> u16 {
            0
        }
    }

    struct DenyAllPermissions;

    impl PermissionEvaluator for DenyAllPermissions {
        fn effective_power(&self, _: UserUid, _: RoomAction) -> u16 {
            0
        }

        fn required_power(&self, _: Uuid, _: RoomAction) -> u16 {
            1
        }
    }

    fn join_voice_room(registry: &mut RoomRegistry, session: &mut Session, room_id: Uuid) {
        registry
            .join_voice_room(session, room_id, &AllowAllPermissions)
            .unwrap();
    }

    #[test]
    fn new_session_has_no_active_voice_room() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let session = Session::new(user_uid, "Ada".to_owned());

        assert_eq!(session.user_uid(), user_uid);
        assert_eq!(session.display_name(), "Ada");
        assert_eq!(session.active_voice_room_id(), None);
    }

    #[test]
    fn new_room_registry_has_no_members() {
        let registry = RoomRegistry::default();

        assert_eq!(registry.members(Uuid::new_v4()), None);
    }

    #[test]
    fn joining_a_voice_room_adds_the_session_to_that_room() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();

        let transition = registry
            .join_voice_room(&mut session, room_id, &AllowAllPermissions)
            .unwrap();

        assert_eq!(transition, VoiceRoomTransition::Joined { room_id });
        assert_eq!(session.active_voice_room_id(), Some(room_id));
        assert!(registry.members(room_id).unwrap().contains(&user_uid));
    }

    #[test]
    fn joining_the_current_voice_room_is_a_no_op() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut session, room_id);

        let transition = registry
            .join_voice_room(&mut session, room_id, &AllowAllPermissions)
            .unwrap();

        assert_eq!(transition, VoiceRoomTransition::NoChange);
        assert_eq!(registry.members(room_id).unwrap().len(), 1);
    }

    #[test]
    fn joining_another_voice_room_switches_rooms() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let first_room_id = Uuid::new_v4();
        let second_room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut session, first_room_id);

        let transition = registry
            .join_voice_room(&mut session, second_room_id, &AllowAllPermissions)
            .unwrap();

        assert_eq!(
            transition,
            VoiceRoomTransition::Switched {
                from_room_id: first_room_id,
                to_room_id: second_room_id,
            }
        );
        assert_eq!(session.active_voice_room_id(), Some(second_room_id));
        assert_eq!(registry.members(first_room_id), None);
        assert!(registry.members(second_room_id).unwrap().contains(&user_uid));
    }

    #[test]
    fn leaving_the_active_voice_room_removes_the_session() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut session, room_id);

        let transition = registry.leave_voice_room(&mut session, room_id);

        assert_eq!(transition, VoiceRoomTransition::Left { room_id });
        assert_eq!(session.active_voice_room_id(), None);
        assert_eq!(registry.members(room_id), None);
    }

    #[test]
    fn stale_or_duplicate_leave_does_not_remove_the_current_voice_room() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let first_room_id = Uuid::new_v4();
        let second_room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut session, first_room_id);
        join_voice_room(&mut registry, &mut session, second_room_id);

        let stale_leave = registry.leave_voice_room(&mut session, first_room_id);
        let leave = registry.leave_voice_room(&mut session, second_room_id);
        let duplicate_leave = registry.leave_voice_room(&mut session, second_room_id);

        assert_eq!(stale_leave, VoiceRoomTransition::NoChange);
        assert_eq!(leave, VoiceRoomTransition::Left {
            room_id: second_room_id
        });
        assert_eq!(duplicate_leave, VoiceRoomTransition::NoChange);
        assert_eq!(session.active_voice_room_id(), None);
        assert_eq!(registry.members(second_room_id), None);
    }

    #[test]
    fn text_messages_use_trusted_sender_data_and_reach_other_room_members() {
        let sender_uid = UserUid::from_bytes([1; 32]);
        let recipient_uid = UserUid::from_bytes([2; 32]);
        let mut sender = Session::new(sender_uid, "Ada".to_owned());
        let mut recipient = Session::new(recipient_uid, "Lin".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut sender, room_id);
        join_voice_room(&mut registry, &mut recipient, room_id);

        let routed = registry
            .route_client_room_message(
                &sender,
                ClientMessage::SendTextMessage {
                    room_id,
                    text_content: shared::RoomTextContent::new("Hello".to_owned()).unwrap(),
                },
                42,
                &AllowAllPermissions,
            )
            .unwrap();

        assert_eq!(routed.recipients(), &HashSet::from([recipient_uid]));
        assert_eq!(
            routed.message(),
            &ServerMessage::BroadcastTextMessage {
                room_id,
                sender_uid,
                sender_name: "Ada".to_owned(),
                text_content: shared::RoomTextContent::new("Hello".to_owned()).unwrap(),
                server_timestamp: 42,
            }
        );
    }

    #[test]
    fn messages_for_a_room_other_than_the_active_room_are_rejected() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        join_voice_room(&mut registry, &mut session, Uuid::new_v4());

        let result = registry.route_client_room_message(
            &session,
            ClientMessage::SendTextMessage {
                room_id: Uuid::new_v4(),
                text_content: shared::RoomTextContent::new("Hello".to_owned()).unwrap(),
            },
            42,
            &AllowAllPermissions,
        );

        assert_eq!(result, Err(RoomRoutingError::SenderNotInActiveVoiceRoom));
    }

    #[test]
    fn sdp_messages_are_forwarded_only_to_members_of_the_active_room() {
        let sender_uid = UserUid::from_bytes([1; 32]);
        let recipient_uid = UserUid::from_bytes([2; 32]);
        let mut sender = Session::new(sender_uid, "Ada".to_owned());
        let mut recipient = Session::new(recipient_uid, "Lin".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut sender, room_id);
        join_voice_room(&mut registry, &mut recipient, room_id);

        let routed = registry
            .route_client_room_message(
                &sender,
                ClientMessage::SendWebRtcSdp {
                    room_id,
                    target_uid: recipient_uid,
                    sdp_type: shared::SdpType::Offer,
                    sdp_raw: "offer-sdp".to_owned(),
                },
                42,
                &AllowAllPermissions,
            )
            .unwrap();

        assert_eq!(routed.recipients(), &HashSet::from([recipient_uid]));
        assert_eq!(
            routed.message(),
            &ServerMessage::ForwardWebRtcSdp {
                room_id,
                sender_uid,
                sdp_type: shared::SdpType::Offer,
                sdp_raw: "offer-sdp".to_owned(),
            }
        );
    }

    #[test]
    fn ice_messages_reject_targets_outside_the_active_room() {
        let sender_uid = UserUid::from_bytes([1; 32]);
        let target_uid = UserUid::from_bytes([2; 32]);
        let mut sender = Session::new(sender_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut sender, room_id);

        let result = registry.route_client_room_message(
            &sender,
            ClientMessage::SendWebRtcIce {
                room_id,
                target_uid,
                candidate: serde_json::json!({ "candidate": "candidate-data" }),
            },
            42,
            &AllowAllPermissions,
        );

        assert_eq!(result, Err(RoomRoutingError::TargetNotInActiveVoiceRoom));
    }

    #[test]
    fn joining_a_room_requires_sufficient_join_power() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();

        let result = registry.join_voice_room(&mut session, room_id, &DenyAllPermissions);

        assert_eq!(result, Err(RoomJoinError::PermissionDenied));
        assert_eq!(session.active_voice_room_id(), None);
        assert_eq!(registry.members(room_id), None);
    }

    #[test]
    fn routing_requires_sufficient_power_for_the_message_action() {
        let user_uid = UserUid::from_bytes([1; 32]);
        let mut session = Session::new(user_uid, "Ada".to_owned());
        let mut registry = RoomRegistry::default();
        let room_id = Uuid::new_v4();
        join_voice_room(&mut registry, &mut session, room_id);

        let result = registry.route_client_room_message(
            &session,
            ClientMessage::SendTextMessage {
                room_id,
                text_content: shared::RoomTextContent::new("Hello".to_owned()).unwrap(),
            },
            42,
            &DenyAllPermissions,
        );

        assert_eq!(
            result,
            Err(RoomRoutingError::PermissionDenied {
                action: RoomAction::Write,
            })
        );
    }
}