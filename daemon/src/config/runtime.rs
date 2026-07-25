use std::{collections::HashMap, path::Path, str::FromStr};

use anyhow::Result;
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};

use crate::surfaces::managed::{ManagedNetworkSurface, NetworkSurfaceStatus};

pub struct RuntimeStatusStore {
    database: SqlitePool,
}

impl RuntimeStatusStore {
    pub async fn open(path: &Path) -> Result<Self> {
        let connection_options =
            SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
                .create_if_missing(true);
        let database = SqlitePool::connect_with(connection_options).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS surface_runtime (
                surface_id TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                last_error TEXT,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
        )
        .execute(&database)
        .await?;
        Ok(Self { database })
    }

    /// Restores the connection status each device was last seen in, so a device that was
    /// unreachable before a restart still reads as unreachable until the first reconnect answers.
    pub async fn apply(&self, devices: &mut [ManagedNetworkSurface]) -> Result<()> {
        let runtime: HashMap<_, _> = sqlx::query_as::<_, (String, String, Option<String>)>(
            "SELECT surface_id, status, last_error FROM surface_runtime",
        )
        .fetch_all(&self.database)
        .await?
        .into_iter()
        .map(|(surface_id, status, last_error)| (surface_id, (status, last_error)))
        .collect();
        for device in devices {
            if let Some((status, last_error)) = runtime.get(&device.surface_id.0) {
                device.status = parse_status(status)?;
                device.last_error = last_error.clone();
            }
        }
        Ok(())
    }

    pub async fn record(
        &self,
        surface_id: String,
        status: NetworkSurfaceStatus,
        last_error: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO surface_runtime (surface_id, status, last_error, updated_at)
             VALUES (?, ?, ?, CURRENT_TIMESTAMP)
             ON CONFLICT(surface_id) DO UPDATE SET
                 status = excluded.status,
                 last_error = excluded.last_error,
                 updated_at = excluded.updated_at",
        )
        .bind(surface_id)
        .bind(status_name(&status))
        .bind(last_error)
        .execute(&self.database)
        .await?;
        Ok(())
    }
}

fn status_name(status: &NetworkSurfaceStatus) -> &'static str {
    match status {
        NetworkSurfaceStatus::Connecting => "connecting",
        NetworkSurfaceStatus::Connected => "connected",
        NetworkSurfaceStatus::Unavailable => "unavailable",
        NetworkSurfaceStatus::Disabled => "disabled",
    }
}

fn parse_status(status: &str) -> Result<NetworkSurfaceStatus> {
    match status {
        "connecting" => Ok(NetworkSurfaceStatus::Connecting),
        "connected" => Ok(NetworkSurfaceStatus::Connected),
        "unavailable" => Ok(NetworkSurfaceStatus::Unavailable),
        "disabled" => Ok(NetworkSurfaceStatus::Disabled),
        _ => anyhow::bail!("unsupported runtime status {status}"),
    }
}
