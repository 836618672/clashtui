//! The dashboard is bundled in the executable; preparing it needs no download.
use anyhow::Result;
use sha2::{Digest, Sha256};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const HTML: &str = include_str!("../../../web/dist/index.html");
pub fn deployment_state() -> serde_json::Value {
    serde_json::json!({"version":VERSION,"framework":"Vue 3 + TypeScript + Vite","builtin":true,"sha256":format!("{:x}",Sha256::digest(HTML.as_bytes()))})
}
pub fn prepare() -> Result<()> {
    anyhow::ensure!(
        HTML.contains("id=\"app\"") && HTML.contains("<script"),
        "Embedded dashboard is missing"
    );
    Ok(())
}
/// The web command can listen elsewhere; clients can override the browser URL.
pub fn url() -> String {
    std::env::var("CLASHTUI_WEB_URL").unwrap_or_else(|_| "http://127.0.0.1:9091/".to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_dashboard_is_available_without_core_or_network() {
        prepare().unwrap();
        assert_eq!(deployment_state()["builtin"], true);
        assert!(!HTML.contains("src=\"/src/main.ts\""));
    }
}
