use reqwest::Url;
use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MissiondConfig {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub device_id: Option<String>,
    /// Held as a raw value because it is read through [`InstanceConfig::secret`], which accepts an
    /// inline string as well as the `{ env = ... }` and `{ file = ... }` forms.
    ///
    /// [`InstanceConfig::secret`]: crate::plugins::instance::InstanceConfig::secret
    #[serde(default)]
    pub key: Option<toml::Value>,
}

/// Where the instance finds its display.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Address(Url),
    /// Whatever address the display with this `device_id` currently announces over mDNS, so a
    /// display that moves to another address is followed without an edit.
    Discovered(String),
    /// Neither is set yet. The instance still starts, because the display picker is answered by
    /// a running instance.
    Unchosen,
}

impl MissiondConfig {
    /// An address wins over a `device_id`, because it is the more specific of the two.
    pub fn target(&self) -> Result<Target, String> {
        let url = self
            .url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty());
        let device_id = self
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|device_id| !device_id.is_empty());
        match (url, device_id) {
            (Some(url), _) => api_url(url).map(Target::Address),
            (None, Some(device_id)) => Ok(Target::Discovered(device_id.to_string())),
            (None, None) => Ok(Target::Unchosen),
        }
    }
}

/// Turns the address a user opens the missiond web UI at into its API root. A pasted `/api` is
/// not doubled, and a path prefix from a reverse proxy is kept.
fn api_url(url: &str) -> Result<Url, String> {
    let trimmed = url.trim_end_matches('/');
    let without_api = trimmed.strip_suffix("/api").unwrap_or(trimmed);

    let mut parsed =
        Url::parse(without_api).map_err(|error| format!("{trimmed} is not an address: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(format!(
            "{trimmed} needs an http:// or https:// scheme, such as http://display.local:3000"
        ));
    }
    parsed
        .path_segments_mut()
        .map_err(|()| format!("{trimmed} has no path"))?
        .pop_if_empty()
        .push("api");
    Ok(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_web_ui_address_becomes_the_api_root() {
        assert_eq!(
            api_url("http://display.local:3000/").unwrap().as_str(),
            "http://display.local:3000/api"
        );
        assert_eq!(
            api_url("https://example.com/lobby/api").unwrap().as_str(),
            "https://example.com/lobby/api"
        );
    }

    #[test]
    fn an_address_without_a_scheme_says_what_is_missing() {
        let reason = api_url("display.local:3000").unwrap_err();

        assert!(reason.contains("http://"), "{reason}");
    }

    #[test]
    fn an_address_wins_over_a_device_id_and_blank_fields_count_as_unset() {
        let both: MissiondConfig =
            toml::from_str("url = \"http://10.0.0.5:3000\"\ndevice_id = \"lobby\"").unwrap();
        let blank_url: MissiondConfig =
            toml::from_str("url = \" \"\ndevice_id = \"lobby\"").unwrap();

        assert_eq!(
            both.target().unwrap(),
            Target::Address(Url::parse("http://10.0.0.5:3000/api").unwrap())
        );
        assert_eq!(
            blank_url.target().unwrap(),
            Target::Discovered("lobby".to_string())
        );
    }

    #[test]
    fn an_unknown_configuration_key_is_rejected_rather_than_ignored() {
        let parsed: Result<MissiondConfig, _> = toml::from_str("url = \"http://x\"\ntoken = \"y\"");

        assert!(parsed.is_err());
    }
}
