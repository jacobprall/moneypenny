use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FleetTemplate {
    #[serde(default)]
    pub agents: Vec<AgentTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentTemplate {
    pub name: String,
    pub persona: Option<String>,
    #[serde(default = "default_trust_level")]
    pub trust_level: String,
    #[serde(default = "default_llm_provider")]
    pub llm_provider: String,
    pub llm_model: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub policies: Vec<serde_json::Value>,
    #[serde(default)]
    pub tools: Vec<ToolTemplate>,
    #[serde(default)]
    pub seed_facts: Vec<FactSeed>,
    #[serde(default)]
    pub seed_knowledge: Vec<KnowledgeSeed>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ToolTemplate {
    pub name: String,
    pub description: String,
    pub script: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FactSeed {
    pub content: String,
    pub summary: Option<String>,
    pub pointer: Option<String>,
    pub keywords: Option<String>,
    #[serde(default = "default_shared_scope")]
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KnowledgeSeed {
    pub title: Option<String>,
    pub path: Option<String>,
    pub content: String,
    pub metadata: Option<String>,
    #[serde(default = "default_shared_scope")]
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyBundle {
    #[serde(default)]
    pub policies: Vec<serde_json::Value>,
    pub signature: Option<PolicyBundleSignature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyBundleSignature {
    #[serde(default = "default_signature_algo")]
    pub algo: String,
    pub value: String,
}

fn default_trust_level() -> String {
    "standard".to_string()
}

fn default_llm_provider() -> String {
    "local".to_string()
}

fn default_shared_scope() -> String {
    "shared".to_string()
}

fn default_signature_algo() -> String {
    "sha256".to_string()
}

pub fn load_fleet_template(path: &str) -> anyhow::Result<FleetTemplate> {
    let content = std::fs::read_to_string(path)?;
    if Path::new(path).extension().and_then(|s| s.to_str()) == Some("toml") {
        Ok(toml::from_str(&content)?)
    } else {
        Ok(serde_json::from_str(&content)?)
    }
}

pub fn load_policy_bundle(path: &str) -> anyhow::Result<PolicyBundle> {
    let content = std::fs::read_to_string(path)?;
    if Path::new(path).extension().and_then(|s| s.to_str()) == Some("toml") {
        Ok(toml::from_str(&content)?)
    } else {
        Ok(serde_json::from_str(&content)?)
    }
}

pub fn verify_policy_bundle_signature(bundle: &PolicyBundle) -> anyhow::Result<()> {
    let Some(sig) = &bundle.signature else {
        anyhow::bail!("policy bundle missing signature");
    };
    if !sig.algo.eq_ignore_ascii_case("sha256") {
        anyhow::bail!("unsupported signature algo '{}'", sig.algo);
    }
    let canonical = serde_json::to_string(&bundle.policies)?;
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    let digest_hex = format!("{:x}", hasher.finalize());
    if digest_hex != sig.value.to_ascii_lowercase() {
        anyhow::bail!("bundle signature mismatch");
    }
    Ok(())
}

pub fn parse_tags_csv(tags_csv: &str) -> Vec<String> {
    let mut set = HashSet::new();
    for t in tags_csv.split(',') {
        let tag = t.trim().to_ascii_lowercase();
        if !tag.is_empty() {
            set.insert(tag);
        }
    }
    let mut tags: Vec<String> = set.into_iter().collect();
    tags.sort();
    tags
}

pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    parse_tags_csv(&tags.join(","))
}

pub fn tags_to_csv(tags: &[String]) -> String {
    normalize_tags(tags).join(",")
}

pub fn matches_scope(tags_csv: Option<&str>, scope: Option<&str>) -> bool {
    let Some(scope_expr) = scope.map(str::trim).filter(|s| !s.is_empty()) else {
        return true;
    };
    let provided = parse_tags_csv(tags_csv.unwrap_or(""));
    let provided_set: HashSet<&str> = provided.iter().map(String::as_str).collect();
    let required = parse_tags_csv(scope_expr);
    required.iter().all(|t| provided_set.contains(t.as_str()))
}
