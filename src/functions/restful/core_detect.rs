use serde::Deserialize;

use super::*;

/// Response from the `/version` endpoint.
///
/// Require Mihomo identity before delivering mutations to the controller.
#[derive(Deserialize)]
pub(super) struct VersionResponse {
    version: String,
    #[serde(default)]
    meta: bool,
}

/// Detect which core is actually running by querying `/version`.
///
/// Only Mihomo controllers are supported.
pub fn detect_core_type() -> Result<crate::config::CoreType> {
    request(Method::Get, "/version", None).and_then(|r| parse_core_type(r.json()?))
}

pub(super) fn parse_core_type(v: VersionResponse) -> Result<crate::config::CoreType> {
    if v.meta && !v.version.trim().is_empty() && !v.version.contains("sing-box") {
        Ok(crate::config::CoreType::Mihomo)
    } else {
        Err(minreq::Error::IoError(std::io::Error::other(
            "The controller is not a supported Mihomo backend",
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_mihomo_from_version_string() {
        let json = r#"{"meta": true, "version": "v1.18.10"}"#;
        let v: VersionResponse = serde_json::from_str(json).unwrap();
        assert!(parse_core_type(v).is_ok());
    }

    #[test]
    fn unsupported_backend_identity_is_rejected() {
        for json in [
            r#"{"version":""}"#,
            r#"{"version":"other"}"#,
            r#"{"meta":true,"version":"sing-box 1.13.11"}"#,
        ] {
            assert!(parse_core_type(serde_json::from_str(json).unwrap()).is_err());
        }
    }
}
