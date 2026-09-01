use std::path::Path;

use rusqlite::Connection;

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
}