use super::{Method, request};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResourceKind {
    Rules,
    ProxyProviders,
    RuleProviders,
}

impl ResourceKind {
    pub fn path(self) -> &'static str {
        match self {
            Self::Rules => "/rules",
            Self::ProxyProviders => "/providers/proxies",
            Self::RuleProviders => "/providers/rules",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Self::Rules => "Rules",
            Self::ProxyProviders => "Proxy providers",
            Self::RuleProviders => "Rule providers",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResourceItem {
    pub id: String,
    pub label: String,
    pub value: Value,
}

pub fn fetch(kind: ResourceKind) -> anyhow::Result<Vec<ResourceItem>> {
    parse(kind, request(Method::Get, kind.path(), None)?.json()?)
}

pub fn parse(kind: ResourceKind, response: Value) -> anyhow::Result<Vec<ResourceItem>> {
    let field = if kind == ResourceKind::Rules {
        "rules"
    } else {
        "providers"
    };
    let value = response
        .get(field)
        .ok_or_else(|| anyhow::anyhow!("Response is missing {field}"))?;
    let mut entries: Vec<(String, Value)> = match value {
        Value::Array(entries) if kind == ResourceKind::Rules => entries
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        Value::Object(entries) => entries
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        _ => anyhow::bail!("Unexpected {field} response format"),
    };
    if kind == ResourceKind::Rules {
        // Numeric keys are API rule indices, not alphabetical display indices.
        entries.sort_by_key(|(index, _)| index.parse::<usize>().unwrap_or(usize::MAX));
    }
    Ok(entries
        .into_iter()
        .map(|(id, value)| {
            let label = if kind == ResourceKind::Rules {
                format!(
                    "{}  {}  {} → {}{}",
                    id,
                    text(&value, "type"),
                    text(&value, "payload"),
                    text(&value, "proxy"),
                    if value["disabled"].as_bool() == Some(true) {
                        " [disabled]"
                    } else {
                        ""
                    }
                )
            } else {
                let count = value["proxies"]
                    .as_array()
                    .map(Vec::len)
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| text(&value, "ruleCount"));
                format!(
                    "{id}  {}  count:{count}  updated:{}",
                    text(&value, "vehicleType"),
                    text(&value, "updatedAt")
                )
            };
            ResourceItem { id, label, value }
        })
        .collect())
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .filter(|v| !v.is_null())
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| v.to_string())
        })
        .unwrap_or_default()
}

pub fn encode_path(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

pub fn update(kind: ResourceKind, id: &str, disabled: Option<bool>) -> anyhow::Result<()> {
    match kind {
        ResourceKind::Rules => {
            let index: usize = id.parse()?;
            let disabled = disabled.ok_or_else(|| {
                anyhow::anyhow!("This backend does not report rule disable state")
            })?;
            let payload = serde_json::json!({ index.to_string(): !disabled });
            request(Method::Patch, "/rules/disable", Some(payload.to_string()))?;
        }
        _ => {
            request(
                Method::Put,
                &format!("{}/{}", kind.path(), encode_path(id)),
                None,
            )?;
        }
    }
    Ok(())
}

pub fn healthcheck(id: &str) -> anyhow::Result<()> {
    request(
        Method::Get,
        &format!("/providers/proxies/{}/healthcheck", encode_path(id)),
        None,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rule_object_preserves_numeric_api_indices() {
        let rows = parse(
            ResourceKind::Rules,
            serde_json::json!({"rules":{"10":{"type":"MATCH"},"2":{"type":"DOMAIN"}}}),
        )
        .unwrap();
        assert_eq!(rows[0].id, "2");
        assert_eq!(rows[1].id, "10");
    }

    #[test]
    fn resource_names_are_one_encoded_path_segment() {
        assert_eq!(encode_path("a/b ?"), "a%2Fb%20%3F");
        assert!(parse(ResourceKind::ProxyProviders, serde_json::json!({})).is_err());
    }
}
