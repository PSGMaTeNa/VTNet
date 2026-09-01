use std::{env, path::PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use shared::UserUid;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:3000";
const DEFAULT_DATABASE_PATH: &str = "vtnet.sqlite3";
const DEFAULT_SERVER_NAME: &str = "VTNet Server";
const DEFAULT_ADMIN_NAME: &str = "Administrator";

/// Startup settings loaded from environment variables with safe local defaults.
pub struct ServerConfig {
    pub bind_address: String,
    pub database_path: PathBuf,
    pub server_name: String,
    pub initial_administrator: Option<InitialAdministrator>,
    pub initial_ram_voice_room_name: Option<String>,
}

pub struct InitialAdministrator {
    pub user_uid: UserUid,
    pub display_name: String,
}

impl ServerConfig {
    pub fn from_environment() -> Result<Self, String> {
        Self::from_values(
            env::var("VTNET_BIND_ADDRESS").ok(),
            env::var("VTNET_DATABASE_PATH").ok(),
            env::var("VTNET_SERVER_NAME").ok(),
            env::var("VTNET_INITIAL_ADMIN_UID").ok(),
            env::var("VTNET_INITIAL_ADMIN_DISPLAY_NAME").ok(),
            env::var("VTNET_INITIAL_RAM_VOICE_ROOM_NAME").ok(),
        )
    }

    fn from_values(
        bind_address: Option<String>,
        database_path: Option<String>,
        server_name: Option<String>,
        administrator_uid: Option<String>,
        administrator_name: Option<String>,
        initial_ram_voice_room_name: Option<String>,
    ) -> Result<Self, String> {
        let initial_administrator = administrator_uid
            .filter(|value| !value.is_empty())
            .map(|encoded_uid| {
                Ok::<InitialAdministrator, String>(InitialAdministrator {
                    user_uid: decode_user_uid(&encoded_uid)?,
                    display_name: administrator_name
                        .filter(|value| !value.is_empty())
                        .unwrap_or_else(|| DEFAULT_ADMIN_NAME.to_owned()),
                })
            })
            .transpose()?;

        Ok(Self {
            bind_address: bind_address.unwrap_or_else(|| DEFAULT_BIND_ADDRESS.to_owned()),
            database_path: PathBuf::from(
                database_path.unwrap_or_else(|| DEFAULT_DATABASE_PATH.to_owned()),
            ),
            server_name: server_name.unwrap_or_else(|| DEFAULT_SERVER_NAME.to_owned()),
            initial_administrator,
            initial_ram_voice_room_name: initial_ram_voice_room_name.filter(|value| !value.is_empty()),
        })
    }
}

fn decode_user_uid(encoded_uid: &str) -> Result<UserUid, String> {
    let bytes = STANDARD
        .decode(encoded_uid)
        .map_err(|_| "VTNET_INITIAL_ADMIN_UID must be valid Base64".to_owned())?;
    let bytes: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "VTNET_INITIAL_ADMIN_UID must decode to exactly 32 bytes".to_owned())?;

    Ok(UserUid::from_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_server_local_without_an_administrator() {
        let config = ServerConfig::from_values(None, None, None, None, None, None).unwrap();

        assert_eq!(config.bind_address, DEFAULT_BIND_ADDRESS);
        assert_eq!(config.database_path, PathBuf::from(DEFAULT_DATABASE_PATH));
        assert!(config.initial_administrator.is_none());
        assert!(config.initial_ram_voice_room_name.is_none());
    }

    #[test]
    fn configured_administrator_uses_the_given_identity_and_name() {
        let encoded_uid = STANDARD.encode([1; 32]);
        let config = ServerConfig::from_values(
            None,
            None,
            None,
            Some(encoded_uid),
            Some("Ada".to_owned()),
            None,
        )
        .unwrap();
        let administrator = config.initial_administrator.unwrap();

        assert_eq!(administrator.user_uid, UserUid::from_bytes([1; 32]));
        assert_eq!(administrator.display_name, "Ada");
    }

    #[test]
    fn configured_initial_voice_room_name_is_preserved() {
        let config = ServerConfig::from_values(
            None,
            None,
            None,
            None,
            None,
            Some("General".to_owned()),
        )
        .unwrap();

        assert_eq!(config.initial_ram_voice_room_name.as_deref(), Some("General"));
    }
}