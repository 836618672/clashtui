//! Verify the configuration features exposed by the Clash API.
//! Credentials and complete provider contents are not exposed.
use crate::config::CoreType;
use anyhow::{Result, ensure};
use serde_json::Value;

pub fn parse(bytes: &[u8], core: CoreType) -> Result<Value> {
    match core {
        CoreType::Mihomo => Ok(serde_json::to_value(serde_yml::from_slice::<
            serde_yml::Value,
        >(bytes)?)?),
    }
}

fn endpoint(value: &str) -> String {
    let value = value.trim_end_matches('/');
    let value = if value.starts_with("http://") || value.starts_with("https://") {
        value.to_owned()
    } else {
        format!("http://{value}")
    };
    value.replace("http://0.0.0.0:", "http://127.0.0.1:")
}

pub fn validate_endpoint(
    config: &Value,
    core: CoreType,
    controller: &str,
    secret: Option<&str>,
) -> Result<()> {
    let (address, credential) = match core {
        CoreType::Mihomo => (&config["external-controller"], &config["secret"]),
    };
    let address = address.as_str().unwrap_or("");
    let credential = credential.as_str().filter(|value| !value.is_empty());
    ensure!(
        endpoint(address) == endpoint(controller)
            && credential == secret.filter(|value| !value.is_empty()),
        "Online activation cannot change the controller address or secret. Migrate the active core file and override configuration while the service is stopped, then restart all clients; see docs/testing/tui_web_testing_zh.md"
    );
    Ok(())
}

pub fn verify(config: &Value, core: CoreType) -> Result<()> {
    use crate::functions::restful::session::CoreSession;
    use minreq::Method;
    let session = CoreSession::current();
    let runtime: Value = session.request(Method::Get, "/configs", None)?.json()?;
    let proxies: Value = session.request(Method::Get, "/proxies", None)?.json()?;
    let rules = if core == CoreType::Mihomo && config["rules"].is_array() {
        Some(
            session
                .request(Method::Get, "/rules", None)?
                .json::<Value>()?,
        )
    } else {
        None
    };
    compare(config, core, &runtime, &proxies, rules.as_ref())
}

pub fn compare(
    config: &Value,
    core: CoreType,
    runtime: &Value,
    proxies: &Value,
    rules: Option<&Value>,
) -> Result<()> {
    let expected_runtime = match core {
        CoreType::Mihomo => config.clone(),
    };
    let fields = [
        "mode",
        "log-level",
        "mixed-port",
        "port",
        "socks-port",
        "redir-port",
        "tproxy-port",
        "allow-lan",
        "ipv6",
        "bind-address",
        "tcp-concurrent",
        "unified-delay",
    ];
    for field in fields {
        if let Some(expected) = expected_runtime.get(field).filter(|value| !value.is_null())
            && let Some(actual) = runtime.get(field).filter(|value| !value.is_null())
        {
            let equal = if field == "mode" {
                expected
                    .as_str()
                    .zip(actual.as_str())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            } else {
                expected == actual
            };
            ensure!(equal, "Activation readback mismatch: {field}");
        }
    }
    let actual = proxies["proxies"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Activation readback has no proxies object"))?;
    let collections: &[&str] = &["proxies", "proxy-groups"];
    for collection in collections {
        if let Some(entries) = config[collection].as_array() {
            for entry in entries {
                let name = entry["name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Generated proxy or group has no name"))?;
                let observed = actual.get(name).ok_or_else(|| {
                    anyhow::anyhow!("Activation readback is missing proxy/group {name}")
                })?;
                let members = &entry["proxies"];
                if let Some(members) = members.as_array() {
                    let observed = observed["all"].as_array().ok_or_else(|| {
                        anyhow::anyhow!("Activation readback has no members for {name}")
                    })?;
                    ensure!(
                        members.iter().all(|member| observed.contains(member)),
                        "Activation readback group membership mismatch: {name}"
                    );
                }
            }
        }
    }
    if let Some(expected) = config["rules"]
        .as_array()
        .filter(|_| core == CoreType::Mihomo)
    {
        let actual = rules
            .and_then(|value| value["rules"].as_array())
            .ok_or_else(|| anyhow::anyhow!("Activation rule readback is unavailable"))?;
        ensure!(
            actual.len() == expected.len(),
            "Activation readback rule count mismatch"
        );
        for (index, (rule, actual)) in expected.iter().zip(actual).enumerate() {
            let rule = rule
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid generated rule"))?;
            let kind = rule.split(',').next().unwrap_or("");
            let normalized = |value: &str| {
                value
                    .chars()
                    .filter(|c| c.is_ascii_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect::<String>()
            };
            let expected_type = normalized(kind);
            let actual_type = normalized(actual["type"].as_str().unwrap_or(""));
            // Mihomo reports both IPv4 and IPv6 CIDR rules as IPCIDR.
            // Payload and target checks below still distinguish their meaning.
            ensure!(
                expected_type == actual_type
                    || (expected_type == "ipcidr6" && actual_type == "ipcidr"),
                "Activation readback rule type mismatch at {index}"
            );
            if !matches!(kind, "AND" | "OR" | "NOT" | "SUB-RULE") {
                let parts: Vec<_> = rule.split(',').collect();
                if kind != "MATCH" && parts.len() >= 3 {
                    let expected_payload = parts[1].trim();
                    let actual_payload = actual["payload"].as_str();
                    // GeoIP's API uses lowercase ISO country codes.
                    let equal = if kind.eq_ignore_ascii_case("GEOIP") {
                        actual_payload
                            .is_some_and(|payload| expected_payload.eq_ignore_ascii_case(payload))
                    } else {
                        actual_payload == Some(expected_payload)
                    };
                    ensure!(
                        equal,
                        "Activation readback rule payload mismatch at {index}"
                    );
                }
                let target = parts
                    .iter()
                    .rev()
                    .find(|part| !matches!(**part, "no-resolve" | "src" | "dst"));
                ensure!(
                    target.is_some_and(|target| actual["proxy"].as_str() == Some(target.trim())),
                    "Activation readback rule target mismatch at {index}"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn success_response_with_old_profile_is_rejected() {
        let expected = json!({"mode":"rule","mixed-port":7890,"proxies":[{"name":"new-node"}],"proxy-groups":[{"name":"group","proxies":["new-node"]}],"rules":["DOMAIN,example.org,group","MATCH,DIRECT"]});
        let runtime = json!({"mode":"rule","mixed-port":7890});
        let proxies = json!({"proxies":{"new-node":{},"group":{"all":["new-node"]}}});
        let rules = json!({"rules":[{"type":"Domain","payload":"example.org","proxy":"group"},{"type":"Match","proxy":"DIRECT"}]});
        compare(
            &expected,
            CoreType::Mihomo,
            &runtime,
            &proxies,
            Some(&rules),
        )
        .unwrap();
        assert!(
            compare(
                &expected,
                CoreType::Mihomo,
                &runtime,
                &json!({"proxies":{"old-node":{}}}),
                Some(&rules)
            )
            .is_err()
        );
        assert!(
            compare(
                &expected,
                CoreType::Mihomo,
                &runtime,
                &proxies,
                Some(&json!({"rules":[]}))
            )
            .is_err()
        );
        assert!(
            compare(
                &expected,
                CoreType::Mihomo,
                &json!({"mode":"global"}),
                &proxies,
                Some(&rules)
            )
            .is_err()
        );
        let mut stale = rules.clone();
        stale["rules"][0]["payload"] = json!("old.example.org");
        assert!(
            compare(
                &expected,
                CoreType::Mihomo,
                &runtime,
                &proxies,
                Some(&stale)
            )
            .is_err()
        );
    }

    #[test]
    fn ipv6_cidr_api_alias_is_accepted_without_relaxing_payload_or_target_checks() {
        let expected = json!({"rules":["IP-CIDR6,2001:db8::/32,DIRECT,no-resolve"]});
        let proxies = json!({"proxies":{}});
        let rules = json!({"rules":[{"type":"IPCIDR","payload":"2001:db8::/32","proxy":"DIRECT"}]});
        compare(
            &expected,
            CoreType::Mihomo,
            &json!({}),
            &proxies,
            Some(&rules),
        )
        .unwrap();
        for (field, wrong) in [
            ("type", "Domain"),
            ("payload", "2001:db9::/32"),
            ("proxy", "REJECT"),
        ] {
            let mut stale = rules.clone();
            stale["rules"][0][field] = json!(wrong);
            assert!(
                compare(
                    &expected,
                    CoreType::Mihomo,
                    &json!({}),
                    &proxies,
                    Some(&stale)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn geoip_country_case_is_normalized_but_different_countries_are_rejected() {
        let expected = json!({"rules":["GEOIP,CN,DIRECT"]});
        let proxies = json!({"proxies":{}});
        let mut rules = json!({"rules":[{"type":"GeoIP","payload":"cn","proxy":"DIRECT"}]});
        compare(
            &expected,
            CoreType::Mihomo,
            &json!({}),
            &proxies,
            Some(&rules),
        )
        .unwrap();
        rules["rules"][0]["payload"] = json!("us");
        assert!(
            compare(
                &expected,
                CoreType::Mihomo,
                &json!({}),
                &proxies,
                Some(&rules)
            )
            .is_err()
        );
    }

    #[test]
    fn changed_endpoint_or_secret_is_rejected_before_online_activation() {
        let config = json!({"external-controller":"0.0.0.0:9090","secret":"old"});
        validate_endpoint(
            &config,
            CoreType::Mihomo,
            "http://127.0.0.1:9090/",
            Some("old"),
        )
        .unwrap();
        assert!(
            validate_endpoint(
                &config,
                CoreType::Mihomo,
                "http://127.0.0.1:9091",
                Some("old")
            )
            .is_err()
        );
        assert!(
            validate_endpoint(
                &config,
                CoreType::Mihomo,
                "http://127.0.0.1:9090",
                Some("new")
            )
            .is_err()
        );
    }
}
