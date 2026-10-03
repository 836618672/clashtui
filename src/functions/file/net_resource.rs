use serde_yml::Value;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub enum ResourceSection {
    ProxyProvider,
    RuleProvider,
}

impl std::fmt::Display for ResourceSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceSection::ProxyProvider => write!(f, "proxy-provider"),
            ResourceSection::RuleProvider => write!(f, "rule-provider"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetResource {
    pub name: String,
    pub url: String,
    pub path: String,
    pub section: ResourceSection,
    pub format: String,
    pub behavior: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct NetResourceUpdate {
    pub name: String,
    pub url: String,
    #[allow(dead_code)]
    pub path: String,
    pub section: ResourceSection,
    pub ok: bool,
    pub error: Option<String>,
}

pub fn format_net_updates(updates: &[NetResourceUpdate]) -> String {
    updates
        .iter()
        .map(|u| {
            let domain = extract_domain(&u.url).unwrap_or(&u.url);
            if u.ok {
                format!("  {} {} {}: ok", u.section, u.name, domain)
            } else {
                format!(
                    "  {} {} {}: FAILED — {}",
                    u.section,
                    u.name,
                    domain,
                    u.error.as_deref().unwrap_or("unknown")
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_domain(url: &str) -> Option<&str> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    rest.split('/').next()
}

pub trait ExtractNetResources {
    fn extract(&self, sections: &[ResourceSection]) -> Vec<NetResource>;
}

impl ExtractNetResources for serde_yml::Mapping {
    fn extract(&self, sections: &[ResourceSection]) -> Vec<NetResource> {
        let mut resources = Vec::new();

        for section in sections {
            let key = match section {
                ResourceSection::ProxyProvider => "proxy-providers",
                ResourceSection::RuleProvider => "rule-providers",
            };

            let section_val = match self.get(Value::String(key.to_string())) {
                Some(Value::Mapping(map)) => map,
                _ => continue,
            };

            for (provider_key, provider_val) in section_val {
                let provider_map = match provider_val.as_mapping() {
                    Some(m) => m,
                    None => continue,
                };

                let name = match provider_key.as_str() {
                    Some(s) => s.to_owned(),
                    None => continue,
                };

                let url = match provider_map
                    .get(Value::String("url".to_string()))
                    .and_then(|v| v.as_str())
                {
                    Some(s) => s.to_owned(),
                    None => continue,
                };

                let path = match provider_map
                    .get(Value::String("path".to_string()))
                    .and_then(|v| v.as_str())
                {
                    Some(s) => s.to_owned(),
                    None => {
                        let hash = format!("{:x}", md5::compute(url.as_bytes()));
                        format!("proxies/{hash}")
                    }
                };

                resources.push(NetResource {
                    name,
                    url,
                    path,
                    section: section.clone(),
                    format: provider_map
                        .get("format")
                        .and_then(Value::as_str)
                        .unwrap_or("yaml")
                        .to_owned(),
                    behavior: provider_map
                        .get("behavior")
                        .and_then(Value::as_str)
                        .unwrap_or("classical")
                        .to_owned(),
                });
            }
        }

        resources
    }
}

/// Resolve every cache access before touching the filesystem. Reject symlinks even
/// when they currently point inside the root, so a later retarget cannot escape.
pub fn cache_path(root: &std::path::Path, relative: &str) -> anyhow::Result<std::path::PathBuf> {
    use std::path::Component;
    anyhow::ensure!(!relative.is_empty(), "Provider cache path is empty");
    let root = root.canonicalize()?;
    let mut path = root.clone();
    for component in std::path::Path::new(relative).components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(part) => path.push(part),
            _ => {
                anyhow::bail!("Provider cache path must stay inside the core directory: {relative}")
            }
        }
        match std::fs::symlink_metadata(&path) {
            Ok(meta) => anyhow::ensure!(
                !meta.file_type().is_symlink(),
                "Provider cache path contains a symlink: {}",
                path.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::ensure!(path != root, "Provider cache path must name a file");
    Ok(path)
}

pub fn validate_cache_paths(mapping: &serde_yml::Mapping) -> anyhow::Result<()> {
    let root = std::path::Path::new(&crate::config::CONFIG.cfg_file.mihomo.core.config_dir);
    for section in ["proxy-providers", "rule-providers"] {
        if let Some(providers) = mapping.get(section).and_then(Value::as_mapping) {
            for provider in providers.values() {
                if let Some(path) = provider.get("path").and_then(Value::as_str) {
                    cache_path(root, path)?;
                }
            }
        }
    }
    Ok(())
}

impl NetResource {
    pub fn validate(&self, bytes: &[u8], destination: &std::path::Path) -> anyhow::Result<()> {
        if self.section == ResourceSection::ProxyProvider || self.format == "yaml" {
            let value: serde_yml::Mapping = serde_yml::from_slice(bytes)?;
            let key = if self.section == ResourceSection::ProxyProvider {
                "proxies"
            } else {
                "payload"
            };
            anyhow::ensure!(
                value.get(key).and_then(Value::as_sequence).is_some(),
                "Provider YAML requires a {key} sequence"
            );
        } else if self.format == "text" {
            std::str::from_utf8(bytes)?;
        } else if self.format == "mrs" {
            anyhow::ensure!(
                matches!(self.behavior.as_str(), "domain" | "ipcidr"),
                "MRS requires domain or ipcidr behavior"
            );
            // Use Mihomo's decoder, including compressed payload integrity, rather
            // than accepting a magic prefix as proof of a valid ruleset.
            let payload = super::activation::TemporaryFile::create_beside(destination, bytes)?;
            let converted = super::activation::TemporaryFile::create_beside(destination, b"")?;
            let output =
                std::process::Command::new(&crate::config::CONFIG.cfg_file.mihomo.core.bin_path)
                    .args(["convert-ruleset", &self.behavior, "mrs"])
                    .arg(&payload.0)
                    .arg(&converted.0)
                    .output()?;
            anyhow::ensure!(
                output.status.success(),
                "Invalid MRS ruleset: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        } else {
            anyhow::bail!("Unsupported rule provider format: {}", self.format);
        }
        Ok(())
    }

    pub fn download(&self, with_proxy: bool) -> NetResourceUpdate {
        let result = (|| -> anyhow::Result<()> {
            let root = std::path::Path::new(&crate::config::CONFIG.cfg_file.mihomo.core.config_dir);
            let destination = cache_path(root, &self.path)?;
            let mut response = crate::functions::restful::download::profile(&self.url, with_proxy)?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut response, &mut bytes)?;
            std::fs::create_dir_all(destination.parent().unwrap())?;
            self.validate(&bytes, &destination)?;
            let destination = cache_path(root, &self.path)?;
            super::activation::atomic_write(&destination, &bytes)
        })();
        NetResourceUpdate {
            name: self.name.clone(),
            url: self.url.clone(),
            path: self.path.clone(),
            section: self.section.clone(),
            ok: result.is_ok(),
            error: result.err().map(|error| format!("{error:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn cache_paths_reject_escape_and_symlink_ancestors() {
        let root = std::env::temp_dir().join(format!("clashtui-paths-{:x}", fastrand::u128(..)));
        std::fs::create_dir_all(&root).unwrap();
        assert!(cache_path(&root, "../outside").is_err());
        assert!(cache_path(&root, "/tmp/outside").is_err());
        assert!(cache_path(&root, ".").is_err());
        assert_eq!(
            cache_path(&root, "./rules/new.yaml").unwrap(),
            root.join("rules/new.yaml")
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), root.join("link")).unwrap();
            assert!(cache_path(&root, "link/new.yaml").is_err());
            assert!(cache_path(&root, "link").is_err());
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provider_formats_preserve_metadata_and_reject_bad_payloads() {
        let yaml: serde_yml::Mapping = serde_yml::from_str(
            "rule-providers: {r: {url: 'http://localhost/rules', format: text, behavior: domain}}",
        )
        .unwrap();
        let resource = &yaml.extract(&[ResourceSection::RuleProvider])[0];
        assert_eq!(resource.format, "text");
        assert_eq!(resource.behavior, "domain");
        assert!(
            resource
                .validate(
                    b"example.com\n+.example.org\n",
                    std::path::Path::new("unused")
                )
                .is_ok()
        );
        assert!(
            resource
                .validate(&[0xff], std::path::Path::new("unused"))
                .is_err()
        );
        let mut resource = resource.clone();
        resource.format = "yaml".into();
        assert!(
            resource
                .validate(b"unexpected: []", std::path::Path::new("unused"))
                .is_err()
        );
        assert!(
            resource
                .validate(b"payload: []", std::path::Path::new("unused"))
                .is_ok()
        );
    }

    fn load_test_yaml() -> serde_yml::Mapping {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/profiles/mihomo/net_resource_test.yaml"
        );
        serde_yml::from_reader(File::open(path).unwrap()).unwrap()
    }

    #[test]
    fn extract_all_sections() {
        let yaml = load_test_yaml();
        let resources = yaml.extract(&[
            ResourceSection::ProxyProvider,
            ResourceSection::RuleProvider,
        ]);
        assert_eq!(resources.len(), 5, "should find 2 PP + 3 RP = 5 resources");
        let pp_count = resources
            .iter()
            .filter(|r| r.section == ResourceSection::ProxyProvider)
            .count();
        assert_eq!(pp_count, 2);

        let rp_count = resources
            .iter()
            .filter(|r| r.section == ResourceSection::RuleProvider)
            .count();
        assert_eq!(rp_count, 3);
    }

    #[test]
    fn filter_proxy_providers_only() {
        let yaml = load_test_yaml();
        let resources = yaml.extract(&[ResourceSection::ProxyProvider]);
        assert_eq!(resources.len(), 2);
        for r in &resources {
            assert_eq!(r.section, ResourceSection::ProxyProvider);
        }
    }

    #[test]
    fn filter_rule_providers_only() {
        let yaml = load_test_yaml();
        let resources = yaml.extract(&[ResourceSection::RuleProvider]);
        assert_eq!(resources.len(), 3);
        for r in &resources {
            assert_eq!(r.section, ResourceSection::RuleProvider);
        }
    }

    #[test]
    fn filter_both_sections() {
        let yaml = load_test_yaml();
        let resources = yaml.extract(&[
            ResourceSection::ProxyProvider,
            ResourceSection::RuleProvider,
        ]);
        assert_eq!(resources.len(), 5);
    }

    #[test]
    fn empty_filter() {
        let yaml = load_test_yaml();
        let resources = yaml.extract(&[]);
        assert!(resources.is_empty());
    }

    #[test]
    fn verify_extracted_fields() {
        let yaml = load_test_yaml();
        let resources = yaml.extract(&[
            ResourceSection::ProxyProvider,
            ResourceSection::RuleProvider,
        ]);

        let pp_dcdn = resources
            .iter()
            .find(|r| r.name == "pp-dcdn")
            .expect("pp-dcdn should exist");
        assert_eq!(pp_dcdn.url, "https://cdn.example.com/dcdn.yaml");
        assert_eq!(pp_dcdn.path, "./proxy-providers/dcdn.yaml");
        assert_eq!(pp_dcdn.section, ResourceSection::ProxyProvider);

        let pp_aws = resources
            .iter()
            .find(|r| r.name == "pp-aws")
            .expect("pp-aws should exist");
        assert_eq!(pp_aws.url, "https://s3.amazonaws.com/bucket/proxies.yaml");
        assert_eq!(pp_aws.path, "./proxy-providers/aws.yaml");
        assert_eq!(pp_aws.section, ResourceSection::ProxyProvider);

        let rp_reject = resources
            .iter()
            .find(|r| r.name == "rp-reject")
            .expect("rp-reject should exist");
        assert_eq!(rp_reject.url, "https://rules.example.org/reject.yaml");
        assert_eq!(rp_reject.path, "./rule-providers/reject.yaml");
        assert_eq!(rp_reject.section, ResourceSection::RuleProvider);

        let rp_ads = resources
            .iter()
            .find(|r| r.name == "rp-ads")
            .expect("rp-ads should exist");
        assert_eq!(rp_ads.url, "https://filters.example.net/ads.yaml");
        assert_eq!(rp_ads.path, "./rule-providers/ads.yaml");
        assert_eq!(rp_ads.section, ResourceSection::RuleProvider);
    }

    #[test]
    fn no_provider_sections() {
        let mut yaml = serde_yml::Mapping::new();
        yaml.insert(
            Value::String("proxies".to_string()),
            Value::Sequence(vec![]),
        );
        let resources = yaml.extract(&[
            ResourceSection::ProxyProvider,
            ResourceSection::RuleProvider,
        ]);
        assert!(resources.is_empty());
    }

    #[test]
    fn provider_section_is_scalar() {
        let mut yaml = serde_yml::Mapping::new();
        yaml.insert(
            Value::String("proxy-providers".to_string()),
            Value::String("not-a-mapping".to_string()),
        );
        let resources = yaml.extract(&[ResourceSection::ProxyProvider]);
        assert!(resources.is_empty());
    }
}
