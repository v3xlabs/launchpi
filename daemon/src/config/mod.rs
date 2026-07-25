pub mod devices;
pub mod panels;
pub mod plugins;
pub mod runtime;
pub mod secret;
pub mod store;
pub mod values;

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Serialize;

pub fn config_directory() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LAUNCHPI_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("launchpi"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".config/launchpi"))
}

pub fn state_directory() -> Result<PathBuf> {
    if let Some(path) = env::var_os("LAUNCHPI_STATE_DIR") {
        return Ok(PathBuf::from(path));
    }
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path).join("launchpi"));
    }
    let home = env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join(".local/state/launchpi"))
}

pub fn write_toml<T: Serialize>(path: &Path, document: &T) -> Result<()> {
    let temporary_path = path.with_extension("toml.tmp");
    fs::write(&temporary_path, toml::to_string_pretty(document)?)?;
    fs::rename(temporary_path, path)?;
    Ok(())
}
