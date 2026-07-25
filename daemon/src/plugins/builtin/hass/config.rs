use serde::Deserialize;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HassConfig {
    #[serde(default)]
    pub url: Option<String>,
    /// Held as a raw value because it is read through [`InstanceConfig::secret`], which accepts an
    /// inline string as well as the `{ env = ... }` and `{ file = ... }` forms.
    #[serde(default)]
    pub token: Option<toml::Value>,
}

/// Turns the address a user reads off their browser into the websocket endpoint. Both schemes are
/// accepted in either notation because a user pastes whichever their installation shows them, and
/// a pasted `/api/websocket` should not end up doubled.
pub fn websocket_url(url: &str) -> Result<String, String> {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("url is required".to_string());
    }

    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| format!("{trimmed} needs a scheme, such as http://homeassistant.local:8123"))?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        other => {
            return Err(format!(
                "{other}:// is not a Home Assistant address; use http:// or https://"
            ))
        }
    };

    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix("/api/websocket").unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!("{trimmed} has no host"));
    }

    Ok(format!("{scheme}://{rest}/api/websocket"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_address_becomes_the_websocket_endpoint() {
        assert_eq!(
            websocket_url("http://homeassistant.local:8123"),
            Ok("ws://homeassistant.local:8123/api/websocket".to_string())
        );
        assert_eq!(
            websocket_url("https://home.example.com/"),
            Ok("wss://home.example.com/api/websocket".to_string())
        );
    }

    #[test]
    fn a_websocket_scheme_is_accepted_as_written() {
        assert_eq!(
            websocket_url("ws://10.0.0.4:8123"),
            Ok("ws://10.0.0.4:8123/api/websocket".to_string())
        );
        assert_eq!(
            websocket_url("wss://home.example.com"),
            Ok("wss://home.example.com/api/websocket".to_string())
        );
    }

    #[test]
    fn a_pasted_endpoint_is_not_doubled() {
        assert_eq!(
            websocket_url("http://homeassistant.local:8123/api/websocket"),
            Ok("ws://homeassistant.local:8123/api/websocket".to_string())
        );
    }

    #[test]
    fn an_address_without_a_scheme_says_what_is_missing() {
        let error = websocket_url("homeassistant.local:8123").expect_err("needs a scheme");
        assert!(error.contains("scheme"), "{error}");
    }

    #[test]
    fn an_unusable_address_is_rejected() {
        assert!(websocket_url("").is_err());
        assert!(websocket_url("ftp://home.example.com").is_err());
        assert!(websocket_url("http://").is_err());
    }

    #[test]
    fn an_unknown_configuration_key_is_rejected_rather_than_ignored() {
        let parsed: Result<HassConfig, _> = toml::from_str("ur1 = \"typo\"");
        assert!(parsed.is_err());
    }
}
