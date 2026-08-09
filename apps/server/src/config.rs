use std::{env, str::FromStr};

const DEFAULT_HOST: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 3004;
const DEFAULT_CORS_ORIGIN: &str = "http://localhost:5174";
const DEFAULT_ROOM_IDLE_TTL_MS: u64 = 600_000;
const DEFAULT_RECONNECT_GRACE_MS: u64 = 90_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub cors_origins: Vec<String>,
    pub trust_proxy: bool,
    pub room_idle_ttl_ms: u64,
    pub reconnect_grace_ms: u64,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_owned(),
            port: DEFAULT_PORT,
            cors_origins: vec![DEFAULT_CORS_ORIGIN.to_owned()],
            trust_proxy: false,
            room_idle_ttl_ms: DEFAULT_ROOM_IDLE_TTL_MS,
            reconnect_grace_ms: DEFAULT_RECONNECT_GRACE_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid environment variable {variable}: {reason}")]
pub struct ConfigError {
    variable: &'static str,
    reason: String,
}

impl ConfigError {
    fn invalid(variable: &'static str, reason: impl Into<String>) -> Self {
        Self {
            variable,
            reason: reason.into(),
        }
    }
}

impl ServerConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let defaults = Self::default();
        let host = env::var("HOST").unwrap_or(defaults.host);
        let port = parse_positive(
            "PORT",
            env::var("PORT").ok(),
            defaults.port,
            "must be an integer from 1 to 65535",
        )?;
        let cors = env::var("CORS_ORIGIN").unwrap_or_else(|_| DEFAULT_CORS_ORIGIN.to_owned());
        let trust_proxy = parse_bool(
            "TRUST_PROXY",
            env::var("TRUST_PROXY").ok(),
            defaults.trust_proxy,
        )?;
        let room_idle_ttl_ms = parse_positive(
            "ROOM_IDLE_TTL_MS",
            env::var("ROOM_IDLE_TTL_MS").ok(),
            defaults.room_idle_ttl_ms,
            "must be a positive integer",
        )?;
        let reconnect_grace_ms = parse_positive(
            "RECONNECT_GRACE_MS",
            env::var("RECONNECT_GRACE_MS").ok(),
            defaults.reconnect_grace_ms,
            "must be a positive integer",
        )?;

        Ok(Self {
            host,
            port,
            cors_origins: cors
                .split(',')
                .map(str::trim)
                .filter(|origin| !origin.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
            trust_proxy,
            room_idle_ttl_ms,
            reconnect_grace_ms,
        })
    }
}

fn parse_bool(
    variable: &'static str,
    raw: Option<String>,
    default: bool,
) -> Result<bool, ConfigError> {
    match raw.as_deref() {
        None => Ok(default),
        Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        Some(_) => Err(ConfigError::invalid(
            variable,
            "must be true, false, 1, or 0",
        )),
    }
}

fn parse_positive<T>(
    variable: &'static str,
    raw: Option<String>,
    default: T,
    reason: &'static str,
) -> Result<T, ConfigError>
where
    T: Default + FromStr + PartialEq,
{
    let Some(raw) = raw else {
        return Ok(default);
    };
    raw.parse()
        .ok()
        .filter(|value| *value != T::default())
        .ok_or_else(|| ConfigError::invalid(variable, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_typescript_server() {
        let config = ServerConfig::default();
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 3004);
        assert_eq!(config.cors_origins, ["http://localhost:5174"]);
        assert!(!config.trust_proxy);
        assert_eq!(config.room_idle_ttl_ms, 600_000);
        assert_eq!(config.reconnect_grace_ms, 90_000);
    }
}
