use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

/// Persistent room categories with separate voice and text-channel behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomType {
    RamVoice,
    E2eeText,
}

impl RoomType {
    fn as_database_value(self) -> &'static str {
        match self {
            Self::RamVoice => "ram_voice",
            Self::E2eeText => "e2ee_text",
        }
    }

    fn from_database_value(value: &str) -> Option<Self> {
        match value {
            "ram_voice" => Some(Self::RamVoice),
            "e2ee_text" => Some(Self::E2eeText),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomDefinition {
    pub room_id: Uuid,
    pub name: String,
    pub room_type: RoomType,
    pub sort_order: i64,
}

/// Reads and writes persistent room definitions without holding volatile presence data.
pub struct RoomRepository<'connection> {
    connection: &'connection Connection,
}

impl<'connection> RoomRepository<'connection> {
    pub fn new(connection: &'connection Connection) -> Self {
        Self { connection }
    }

    pub fn create(
        &self,
        room_id: Uuid,
        name: &str,
        room_type: RoomType,
        sort_order: i64,
    ) -> rusqlite::Result<()> {
        self.connection.execute(
            "INSERT INTO rooms (room_id, name, room_type, sort_order) VALUES (?1, ?2, ?3, ?4)",
            params![room_id.to_string(), name, room_type.as_database_value(), sort_order],
        )?;

        Ok(())
    }

    pub fn find(&self, room_id: Uuid) -> rusqlite::Result<Option<RoomDefinition>> {
        let room = self.connection
            .query_row(
                "SELECT room_id, name, room_type, sort_order FROM rooms WHERE room_id = ?1",
                params![room_id.to_string()],
                |row| {
                    let room_id: String = row.get(0)?;
                    let room_type: String = row.get(2)?;
                    Ok((room_id, row.get(1)?, room_type, row.get(3)?))
                },
            )
            .optional()?
            .map(|(room_id, name, room_type, sort_order)| {
                let room_id = Uuid::parse_str(&room_id).expect("rooms.room_id must contain a UUID");
                let room_type = RoomType::from_database_value(&room_type)
                    .expect("rooms.room_type must satisfy the schema constraint");

                RoomDefinition {
                    room_id,
                    name,
                    room_type,
                    sort_order,
                }
            });

        Ok(room)
    }
}

#[cfg(test)]
mod tests {
    use crate::database::Database;

    use super::*;

    #[test]
    fn repository_reads_existing_room_definitions() {
        let database = Database::open_in_memory().unwrap();
        database.initialize_schema().unwrap();
        let repository = RoomRepository::new(database.connection());
        let room_id = Uuid::new_v4();

        repository
            .create(room_id, "General", RoomType::RamVoice, 10)
            .unwrap();

        assert_eq!(
            repository.find(room_id).unwrap(),
            Some(RoomDefinition {
                room_id,
                name: "General".to_owned(),
                room_type: RoomType::RamVoice,
                sort_order: 10,
            })
        );
    }

    #[test]
    fn repository_preserves_e2ee_text_as_a_non_voice_room_type() {
        let database = Database::open_in_memory().unwrap();
        database.initialize_schema().unwrap();
        let repository = RoomRepository::new(database.connection());
        let room_id = Uuid::new_v4();

        repository
            .create(room_id, "Staff chat", RoomType::E2eeText, 20)
            .unwrap();

        assert_eq!(repository.find(room_id).unwrap().unwrap().room_type, RoomType::E2eeText);
    }

    #[test]
    fn repository_returns_none_for_an_unknown_room() {
        let database = Database::open_in_memory().unwrap();
        database.initialize_schema().unwrap();
        let repository = RoomRepository::new(database.connection());

        assert_eq!(repository.find(Uuid::new_v4()).unwrap(), None);
    }
}