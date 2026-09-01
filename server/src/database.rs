use std::path::Path;

use rusqlite::{params, Connection};
use shared::UserUid;

const ADMIN_ROLE_ID: &str = "admin";
const ADMIN_POWER: u16 = 100;
const POWER_ACTIONS: [&str; 5] = ["join", "speak", "write", "signal", "manage_room"];

/// Owns the SQLite connection used for persistent server structure.
pub struct Database {
    connection: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> rusqlite::Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn initialize_schema(&self) -> rusqlite::Result<()> {
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS server_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                server_name TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS trusted_users (
                user_uid BLOB PRIMARY KEY CHECK (length(user_uid) = 32),
                display_name TEXT NOT NULL COLLATE NOCASE UNIQUE,
                is_banned INTEGER NOT NULL DEFAULT 0 CHECK (is_banned IN (0, 1)),
                joined_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS roles (
                role_id TEXT PRIMARY KEY,
                name TEXT NOT NULL COLLATE NOCASE UNIQUE
            );

            CREATE TABLE IF NOT EXISTS user_roles (
                user_uid BLOB NOT NULL,
                role_id TEXT NOT NULL,
                PRIMARY KEY (user_uid, role_id),
                FOREIGN KEY (user_uid) REFERENCES trusted_users(user_uid) ON DELETE CASCADE,
                FOREIGN KEY (role_id) REFERENCES roles(role_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS rooms (
                room_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                room_type TEXT NOT NULL CHECK (room_type IN ('ram_voice', 'e2ee_text')),
                sort_order INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS room_roles (
                room_id TEXT NOT NULL,
                role_id TEXT NOT NULL,
                can_read INTEGER NOT NULL DEFAULT 1 CHECK (can_read IN (0, 1)),
                can_write INTEGER NOT NULL DEFAULT 1 CHECK (can_write IN (0, 1)),
                PRIMARY KEY (room_id, role_id),
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
                FOREIGN KEY (role_id) REFERENCES roles(role_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS role_powers (
                role_id TEXT NOT NULL,
                action TEXT NOT NULL CHECK (action IN ('join', 'speak', 'write', 'signal', 'manage_room')),
                power INTEGER NOT NULL CHECK (power >= 0),
                PRIMARY KEY (role_id, action),
                FOREIGN KEY (role_id) REFERENCES roles(role_id) ON DELETE CASCADE
            );

            CREATE TABLE IF NOT EXISTS room_power_requirements (
                room_id TEXT NOT NULL,
                action TEXT NOT NULL CHECK (action IN ('join', 'speak', 'write', 'signal', 'manage_room')),
                required_power INTEGER NOT NULL CHECK (required_power >= 0),
                PRIMARY KEY (room_id, action),
                FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
            );

            INSERT OR IGNORE INTO server_config (id, server_name) VALUES (1, 'VTNet Server');
            ",
        )
    }

    pub fn set_server_name(&self, server_name: &str) -> rusqlite::Result<()> {
        self.connection.execute(
            "UPDATE server_config SET server_name = ?1 WHERE id = 1",
            params![server_name],
        )?;

        Ok(())
    }

    /// Assigns the configured identity the initial administrator role during server setup.
    pub fn bootstrap_administrator(
        &mut self,
        user_uid: UserUid,
        display_name: &str,
    ) -> rusqlite::Result<()> {
        let transaction = self.connection.transaction()?;

        transaction.execute(
            "INSERT OR IGNORE INTO trusted_users (user_uid, display_name) VALUES (?1, ?2)",
            params![user_uid.as_bytes(), display_name],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO roles (role_id, name) VALUES (?1, 'Administrator')",
            params![ADMIN_ROLE_ID],
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO user_roles (user_uid, role_id) VALUES (?1, ?2)",
            params![user_uid.as_bytes(), ADMIN_ROLE_ID],
        )?;

        for action in POWER_ACTIONS {
            transaction.execute(
                "INSERT OR IGNORE INTO role_powers (role_id, action, power) VALUES (?1, ?2, ?3)",
                params![ADMIN_ROLE_ID, action, ADMIN_POWER],
            )?;
        }

        transaction.commit()
    }

    fn from_connection(connection: Connection) -> rusqlite::Result<Self> {
        // SQLite does not enforce foreign keys unless every connection enables them explicitly.
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;

        Ok(Self { connection })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_enforces_foreign_keys() {
        let database = Database::open_in_memory().unwrap();
        database
            .connection()
            .execute_batch(
                "
                CREATE TABLE parent (id INTEGER PRIMARY KEY);
                CREATE TABLE child (
                    parent_id INTEGER NOT NULL,
                    FOREIGN KEY (parent_id) REFERENCES parent(id)
                );
                ",
            )
            .unwrap();

        let result = database
            .connection()
            .execute("INSERT INTO child (parent_id) VALUES (1)", []);

        assert!(result.is_err());
    }

    #[test]
    fn schema_initialization_is_idempotent_and_creates_the_required_tables() {
        let database = Database::open_in_memory().unwrap();

        database.initialize_schema().unwrap();
        database.initialize_schema().unwrap();

        let mut statement = database
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .unwrap();
        let table_names = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();

        for table_name in [
            "server_config",
            "trusted_users",
            "roles",
            "user_roles",
            "rooms",
            "room_roles",
            "role_powers",
            "room_power_requirements",
        ] {
            assert!(table_names.contains(&table_name.to_owned()));
        }

        let config_count: i64 = database
            .connection()
            .query_row("SELECT COUNT(*) FROM server_config", [], |row| row.get(0))
            .unwrap();
        assert_eq!(config_count, 1);
    }

    #[test]
    fn administrator_bootstrap_assigns_the_configured_identity_maximum_powers() {
        let mut database = Database::open_in_memory().unwrap();
        database.initialize_schema().unwrap();
        let administrator_uid = UserUid::from_bytes([7; 32]);

        database
            .bootstrap_administrator(administrator_uid, "Ada")
            .unwrap();
        database
            .bootstrap_administrator(administrator_uid, "Ada")
            .unwrap();

        let assigned_role_count: i64 = database
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM user_roles WHERE user_uid = ?1 AND role_id = ?2",
                params![administrator_uid.as_bytes(), ADMIN_ROLE_ID],
                |row| row.get(0),
            )
            .unwrap();
        let highest_power: i64 = database
            .connection()
            .query_row(
                "SELECT MIN(power) FROM role_powers WHERE role_id = ?1",
                params![ADMIN_ROLE_ID],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(assigned_role_count, 1);
        assert_eq!(highest_power, i64::from(ADMIN_POWER));
    }
}