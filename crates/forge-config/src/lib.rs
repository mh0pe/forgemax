#![warn(missing_docs)]

//! # forge-config
//!
//! Configuration loading for the Forgemax Code Mode MCP Gateway.
//!
//! Supports TOML configuration files with environment variable expansion
//! and config file includes (local and remote).
//!
//! ## Example
//!
//! ```toml
//! [servers.narsil]
//! command = "narsil-mcp"
//! args = ["--repos", "."]
//! transport = "stdio"
//!
//! [servers.github]
//! url = "https://mcp.github.com/mcp"
//! transport = "sse"
//! headers = { Authorization = "Bearer ${GITHUB_TOKEN}" }
//!
//! [sandbox]
//! timeout_secs = 5
//! max_heap_mb = 64
//! max_concurrent = 8
//! max_tool_calls = 50
//! ```
//!
//! ## Includes
//!
//! Config files can include other config files using `[[include]]`:
//!
//! ```toml
//! [[include]]
//! path = "./shared-servers.toml"
//!
//! [[include]]
//! path = "https://example.com/team-config.toml"
//! sha512 = "abc123..."
//! ```
//!
//! ## Security Modes
//!
//! - `auto-pin` (default): Remote includes without a `sha512` hash are allowed
//!   but a warning is logged with the computed hash for pinning.
//! - `strict`: All remote includes must have a valid `sha512` hash.

#[cfg(feature = "config-watch")]
pub mod watcher;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use thiserror::Error;

/// Errors from config parsing.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// Failed to read config file.
    #[error("failed to read config file: {0}")]
    Io(#[from] std::io::Error),

    /// Failed to parse TOML.
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),

    /// Invalid configuration value.
    #[error("invalid config: {0}")]
    Invalid(String),

    /// Failed to fetch remote include.
    #[error("include error: {0}")]
    Include(String),
}

/// An entry in the `[[include]]` array.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct IncludeEntry {
    /// Path or URL to include. Supports:
    /// - Relative path (resolved relative to the including config file)
    /// - Absolute path
    /// - `file:///path` URI
    /// - `https://url` URI
    /// - `github://owner/repo/path` (resolves to raw.githubusercontent.com, defaults to `main`)
    /// - `github://owner/repo@ref/path` (specific branch/tag/SHA)
    pub path: String,

    /// SHA-512 hash of the remote content for integrity verification.
    /// Required when `security_mode = "strict"` for remote includes.
    #[serde(default)]
    pub sha512: Option<String>,
}

/// Top-level Forge configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ForgeConfig {
    /// Security mode for remote includes.
    /// - `"auto-pin"` (default): allow remote includes without hashes, log computed hash.
    /// - `"strict"`: require SHA-512 hashes for all remote includes.
    #[serde(default = "default_security_mode")]
    pub security_mode: String,

    /// Config file includes. Processed in order; servers and groups from
    /// includes are merged (main config wins on key conflicts).
    #[serde(default)]
    pub include: Vec<IncludeEntry>,

    /// Downstream MCP server configurations, keyed by server name.
    #[serde(default)]
    pub servers: HashMap<String, ServerConfig>,

    /// Sandbox execution settings.
    #[serde(default)]
    pub sandbox: SandboxOverrides,

    /// Server group definitions for cross-server data flow policies.
    #[serde(default)]
    pub groups: HashMap<String, GroupConfig>,

    /// Manifest refresh behavior.
    #[serde(default)]
    pub manifest: ManifestConfig,
}

fn default_security_mode() -> String {
    "auto-pin".to_string()
}

/// Configuration for manifest refresh behavior.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ManifestConfig {
    /// How often to re-discover tools from downstream servers (seconds).
    /// 0 or absent = disabled (manifest is static after startup).
    #[serde(default)]
    pub refresh_interval_secs: Option<u64>,
}

/// Configuration for a server group.
#[derive(Debug, Clone, Deserialize)]
pub struct GroupConfig {
    /// Server names belonging to this group.
    pub servers: Vec<String>,

    /// Isolation mode: "strict" (no cross-group data flow) or "open" (unrestricted).
    #[serde(default = "default_isolation")]
    pub isolation: String,
}

fn default_isolation() -> String {
    "open".to_string()
}

/// Configuration for a single downstream MCP server.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Transport type: "stdio" or "sse".
    pub transport: String,

    /// Command to execute (stdio transport).
    #[serde(default)]
    pub command: Option<String>,

    /// Command arguments (stdio transport).
    #[serde(default)]
    pub args: Vec<String>,

    /// Server URL (sse transport).
    #[serde(default)]
    pub url: Option<String>,

    /// HTTP headers (sse transport).
    #[serde(default)]
    pub headers: HashMap<String, String>,

    /// Server description (optional, for manifest).
    #[serde(default)]
    pub description: Option<String>,

    /// Per-server timeout in seconds for individual tool calls.
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Enable circuit breaker for this server.
    #[serde(default)]
    pub circuit_breaker: Option<bool>,

    /// Number of consecutive failures before opening the circuit (default: 3).
    #[serde(default)]
    pub failure_threshold: Option<u32>,

    /// Seconds to wait before probing a tripped circuit (default: 30).
    #[serde(default)]
    pub recovery_timeout_secs: Option<u64>,
}

/// Sandbox configuration overrides.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SandboxOverrides {
    /// Execution timeout in seconds.
    #[serde(default)]
    pub timeout_secs: Option<u64>,

    /// Maximum V8 heap size in megabytes.
    #[serde(default)]
    pub max_heap_mb: Option<usize>,

    /// Maximum concurrent sandbox executions.
    #[serde(default)]
    pub max_concurrent: Option<usize>,

    /// Maximum tool calls per execution.
    #[serde(default)]
    pub max_tool_calls: Option<usize>,

    /// Execution mode: "in_process" (default) or "child_process".
    #[serde(default)]
    pub execution_mode: Option<String>,

    /// Maximum IPC message size in megabytes (default: 8 MB).
    #[serde(default)]
    pub max_ipc_message_size_mb: Option<usize>,

    /// Maximum resource content size in megabytes (default: 64 MB).
    #[serde(default)]
    pub max_resource_size_mb: Option<usize>,

    /// Maximum concurrent calls in forge.parallel() (default: 8).
    #[serde(default)]
    pub max_parallel: Option<usize>,

    /// Maximum number of servers to connect to concurrently at startup.
    /// Defaults to half the number of CPU cores available.
    #[serde(default)]
    pub startup_concurrency: Option<usize>,

    /// Stash configuration overrides.
    #[serde(default)]
    pub stash: Option<StashOverrides>,

    /// Worker pool configuration overrides.
    #[serde(default)]
    pub pool: Option<PoolOverrides>,
}

/// Configuration overrides for the worker pool.
///
/// When enabled, warm worker processes are reused across executions
/// instead of spawning a new process each time (~5-10ms vs ~50ms).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PoolOverrides {
    /// Enable the worker pool (default: false).
    #[serde(default)]
    pub enabled: Option<bool>,

    /// Minimum number of warm workers to keep ready (default: 2).
    #[serde(default)]
    pub min_workers: Option<usize>,

    /// Maximum number of workers in the pool (default: 8).
    #[serde(default)]
    pub max_workers: Option<usize>,

    /// Kill idle workers after this many seconds (default: 60).
    #[serde(default)]
    pub max_idle_secs: Option<u64>,

    /// Recycle a worker after this many executions (default: 50).
    #[serde(default)]
    pub max_uses: Option<u32>,
}

/// Configuration overrides for the ephemeral stash.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct StashOverrides {
    /// Maximum number of stash entries per session.
    #[serde(default)]
    pub max_keys: Option<usize>,

    /// Maximum size of a single stash value in megabytes.
    #[serde(default)]
    pub max_value_size_mb: Option<usize>,

    /// Maximum total stash size in megabytes.
    #[serde(default)]
    pub max_total_size_mb: Option<usize>,

    /// Default TTL for stash entries in seconds.
    #[serde(default)]
    pub default_ttl_secs: Option<u64>,

    /// Maximum TTL for stash entries in seconds.
    #[serde(default)]
    pub max_ttl_secs: Option<u64>,

    /// Maximum stash operations per execution (None = unlimited).
    #[serde(default)]
    pub max_calls: Option<usize>,
}

impl ForgeConfig {
    /// Parse a config from a TOML string.
    ///
    /// Includes are **not** processed in this path because there is no
    /// file context to resolve relative paths. Use [`from_file`] or
    /// [`from_file_with_env`] to process includes.
    pub fn from_toml(toml_str: &str) -> Result<Self, ConfigError> {
        let config: ForgeConfig = toml::from_str(toml_str)?;
        config.validate()?;
        Ok(config)
    }

    /// Load config from a file path. Processes `[[include]]` entries.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: ForgeConfig = toml::from_str(&content)?;
        config.process_includes(path)?;
        config.validate()?;
        Ok(config)
    }

    /// Parse a config from a TOML string, expanding `${ENV_VAR}` references.
    ///
    /// Includes are **not** processed in this path. Use [`from_file_with_env`].
    pub fn from_toml_with_env(toml_str: &str) -> Result<Self, ConfigError> {
        let expanded = expand_env_vars(toml_str);
        Self::from_toml(&expanded)
    }

    /// Load config from a file path, expanding environment variables.
    /// Processes `[[include]]` entries.
    pub fn from_file_with_env(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let expanded = expand_env_vars(&content);
        let mut config: ForgeConfig = toml::from_str(&expanded)?;
        config.process_includes(path)?;
        config.validate()?;
        Ok(config)
    }

    /// Merge servers and groups from another config. Self wins on key conflicts.
    fn merge_from(&mut self, other: ForgeConfig) {
        for (name, server) in other.servers {
            self.servers.entry(name).or_insert(server);
        }
        for (name, group) in other.groups {
            self.groups.entry(name).or_insert(group);
        }
    }

    /// Process `[[include]]` entries, loading and merging included configs.
    /// Nested includes (includes within included files) are not followed.
    fn process_includes(&mut self, config_path: &Path) -> Result<(), ConfigError> {
        if self.include.is_empty() {
            return Ok(());
        }

        let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        let entries = std::mem::take(&mut self.include);

        for entry in &entries {
            let content = load_include_content(entry, base_dir, &self.security_mode)?;
            let expanded = expand_env_vars(&content);
            let included: ForgeConfig = toml::from_str(&expanded).map_err(|e| {
                ConfigError::Include(format!("failed to parse include '{}': {}", entry.path, e))
            })?;
            self.merge_from(included);
        }

        // Restore the include entries (for informational purposes)
        self.include = entries;
        Ok(())
    }

    fn validate(&self) -> Result<(), ConfigError> {
        // Validate security_mode
        match self.security_mode.as_str() {
            "auto-pin" | "strict" => {}
            other => {
                return Err(ConfigError::Invalid(format!(
                    "unsupported security_mode '{}', supported: auto-pin, strict",
                    other
                )));
            }
        }

        for (name, server) in &self.servers {
            match server.transport.as_str() {
                "stdio" => {
                    if server.command.is_none() {
                        return Err(ConfigError::Invalid(format!(
                            "server '{}': stdio transport requires 'command'",
                            name
                        )));
                    }
                }
                "sse" => {
                    if server.url.is_none() {
                        return Err(ConfigError::Invalid(format!(
                            "server '{}': sse transport requires 'url'",
                            name
                        )));
                    }
                }
                other => {
                    return Err(ConfigError::Invalid(format!(
                        "server '{}': unsupported transport '{}', supported: stdio, sse",
                        name, other
                    )));
                }
            }
        }

        // Validate groups
        let mut seen_servers: HashMap<&str, &str> = HashMap::new();
        for (group_name, group_config) in &self.groups {
            // Validate isolation mode
            match group_config.isolation.as_str() {
                "strict" | "open" => {}
                other => {
                    return Err(ConfigError::Invalid(format!(
                        "group '{}': unsupported isolation '{}', supported: strict, open",
                        group_name, other
                    )));
                }
            }

            for server_ref in &group_config.servers {
                // Check server exists
                if !self.servers.contains_key(server_ref) {
                    return Err(ConfigError::Invalid(format!(
                        "group '{}': references unknown server '{}'",
                        group_name, server_ref
                    )));
                }
                // Check no server in multiple groups
                if let Some(existing_group) = seen_servers.get(server_ref.as_str()) {
                    return Err(ConfigError::Invalid(format!(
                        "server '{}' is in multiple groups: '{}' and '{}'",
                        server_ref, existing_group, group_name
                    )));
                }
                seen_servers.insert(server_ref, group_name);
            }
        }

        // Validate sandbox v0.2 fields
        self.validate_sandbox_v2()?;

        Ok(())
    }

    fn validate_sandbox_v2(&self) -> Result<(), ConfigError> {
        // CV-01: max_resource_size_mb must be > 0 and <= 512
        if let Some(size) = self.sandbox.max_resource_size_mb {
            if size == 0 || size > 512 {
                return Err(ConfigError::Invalid(
                    "sandbox.max_resource_size_mb must be > 0 and <= 512".into(),
                ));
            }
        }

        // CV-02: max_parallel must be >= 1 and <= max_concurrent (or default 8)
        if let Some(parallel) = self.sandbox.max_parallel {
            let max_concurrent = self.sandbox.max_concurrent.unwrap_or(8);
            if parallel < 1 || parallel > max_concurrent {
                return Err(ConfigError::Invalid(format!(
                    "sandbox.max_parallel must be >= 1 and <= max_concurrent ({})",
                    max_concurrent
                )));
            }
        }

        // CV-08: startup_concurrency must be >= 1
        if let Some(concurrency) = self.sandbox.startup_concurrency {
            if concurrency == 0 {
                return Err(ConfigError::Invalid(
                    "sandbox.startup_concurrency must be >= 1".into(),
                ));
            }
        }

        if let Some(ref stash) = self.sandbox.stash {
            // CV-03: stash.max_value_size_mb must be > 0 and <= 256
            if let Some(size) = stash.max_value_size_mb {
                if size == 0 || size > 256 {
                    return Err(ConfigError::Invalid(
                        "sandbox.stash.max_value_size_mb must be > 0 and <= 256".into(),
                    ));
                }
            }

            // CV-04: stash.max_total_size_mb must be >= stash.max_value_size_mb
            if let (Some(total), Some(value)) = (stash.max_total_size_mb, stash.max_value_size_mb) {
                if total < value {
                    return Err(ConfigError::Invalid(
                        "sandbox.stash.max_total_size_mb must be >= sandbox.stash.max_value_size_mb"
                            .into(),
                    ));
                }
            }

            // CV-05: stash.default_ttl_secs must be > 0 and <= stash.max_ttl_secs
            if let Some(default_ttl) = stash.default_ttl_secs {
                if default_ttl == 0 {
                    return Err(ConfigError::Invalid(
                        "sandbox.stash.default_ttl_secs must be > 0".into(),
                    ));
                }
                let max_ttl = stash.max_ttl_secs.unwrap_or(86400);
                if default_ttl > max_ttl {
                    return Err(ConfigError::Invalid(format!(
                        "sandbox.stash.default_ttl_secs ({}) must be <= max_ttl_secs ({})",
                        default_ttl, max_ttl
                    )));
                }
            }

            // CV-06: stash.max_ttl_secs must be > 0 and <= 604800 (7 days)
            if let Some(max_ttl) = stash.max_ttl_secs {
                if max_ttl == 0 || max_ttl > 604800 {
                    return Err(ConfigError::Invalid(
                        "sandbox.stash.max_ttl_secs must be > 0 and <= 604800 (7 days)".into(),
                    ));
                }
            }
        }

        // CV-07: max_resource_size_mb + 1 must fit within IPC message size
        // In child_process mode, resource content flows over IPC
        if let Some(resource_mb) = self.sandbox.max_resource_size_mb {
            let ipc_limit_mb = self.sandbox.max_ipc_message_size_mb.unwrap_or(8); // default 8 MB
            if resource_mb + 1 > ipc_limit_mb {
                return Err(ConfigError::Invalid(format!(
                    "sandbox.max_resource_size_mb ({}) + 1 MB overhead exceeds IPC message limit ({} MB)",
                    resource_mb, ipc_limit_mb
                )));
            }
        }

        // Validate pool config
        if let Some(ref pool) = self.sandbox.pool {
            self.validate_pool(pool)?;
        }

        Ok(())
    }

    fn validate_pool(&self, pool: &PoolOverrides) -> Result<(), ConfigError> {
        let max_concurrent = self.sandbox.max_concurrent.unwrap_or(8);

        // CV-08: max_workers must be >= 1 and <= max_concurrent
        if let Some(max) = pool.max_workers {
            if max == 0 || max > max_concurrent {
                return Err(ConfigError::Invalid(format!(
                    "sandbox.pool.max_workers must be >= 1 and <= max_concurrent ({})",
                    max_concurrent
                )));
            }
        }

        let max_workers = pool.max_workers.unwrap_or(8);

        // CV-09: min_workers must be >= 0 and <= max_workers
        if let Some(min) = pool.min_workers {
            if min > max_workers {
                return Err(ConfigError::Invalid(format!(
                    "sandbox.pool.min_workers ({}) must be <= max_workers ({})",
                    min, max_workers
                )));
            }
        }

        // CV-10: max_uses must be > 0
        if let Some(uses) = pool.max_uses {
            if uses == 0 {
                return Err(ConfigError::Invalid(
                    "sandbox.pool.max_uses must be > 0".into(),
                ));
            }
        }

        // CV-11: max_idle_secs must be >= 5 and <= 3600
        if let Some(idle) = pool.max_idle_secs {
            if !(5..=3600).contains(&idle) {
                return Err(ConfigError::Invalid(
                    "sandbox.pool.max_idle_secs must be >= 5 and <= 3600".into(),
                ));
            }
        }

        Ok(())
    }
}

/// Returns the default startup concurrency: half the number of CPU cores, minimum 1.
///
/// This value is used when `sandbox.startup_concurrency` is not configured.
/// It provides a sensible default that balances startup speed with system load.
pub fn default_startup_concurrency() -> usize {
    let cores = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(2);
    (cores / 2).max(1)
}

/// Expand `${ENV_VAR}` patterns in a string using environment variables.
fn expand_env_vars(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            for c in chars.by_ref() {
                if c == '}' {
                    break;
                }
                var_name.push(c);
            }
            match std::env::var(&var_name) {
                Ok(value) => result.push_str(&value),
                Err(_) => {
                    // Leave the placeholder if env var not found
                    result.push_str(&format!("${{{}}}", var_name));
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Check whether an include path refers to a remote resource.
fn is_remote_include(path: &str) -> bool {
    path.starts_with("https://")
        || path.starts_with("http://")
        || path.starts_with("github://")
}

/// Resolve a `github://` URI to an HTTPS URL for raw content.
///
/// Supported formats:
/// - `github://owner/repo/path/to/file` (defaults to `main` branch)
/// - `github://owner/repo@ref/path/to/file` (specific branch, tag, or SHA)
fn resolve_github_uri(uri: &str) -> Result<String, ConfigError> {
    let remainder = uri.strip_prefix("github://").ok_or_else(|| {
        ConfigError::Include(format!("invalid github URI: {}", uri))
    })?;

    // Split into owner, repo (possibly with @ref), and path
    let parts: Vec<&str> = remainder.splitn(3, '/').collect();
    if parts.len() < 3 {
        return Err(ConfigError::Include(format!(
            "github URI must be github://owner/repo/path: {}",
            uri
        )));
    }

    let owner = parts[0];
    let (repo, git_ref) = if let Some((r, reference)) = parts[1].split_once('@') {
        (r, reference)
    } else {
        (parts[1], "main")
    };
    let path = parts[2];

    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/{}/{}",
        owner, repo, git_ref, path
    ))
}

/// Compute the SHA-512 hash of content, returning it as a lowercase hex string.
fn compute_sha512(content: &str) -> String {
    let mut hasher = Sha512::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

/// Fetch content from a remote URL.
fn fetch_remote(url: &str) -> Result<String, ConfigError> {
    let response = ureq::get(url).call().map_err(|e| {
        ConfigError::Include(format!("failed to fetch '{}': {}", url, e))
    })?;
    let body = response.into_string().map_err(|e| {
        ConfigError::Include(format!("failed to read response from '{}': {}", url, e))
    })?;
    Ok(body)
}

/// Load content for an include entry, handling local files and remote URLs.
fn load_include_content(
    entry: &IncludeEntry,
    base_dir: &Path,
    security_mode: &str,
) -> Result<String, ConfigError> {
    let path_str = &entry.path;

    // Handle file:// URI by stripping the scheme
    if let Some(file_path) = path_str.strip_prefix("file://") {
        let p = PathBuf::from(file_path);
        return std::fs::read_to_string(&p).map_err(|e| {
            ConfigError::Include(format!("failed to read '{}': {}", p.display(), e))
        });
    }

    // Remote includes: https:// or github://
    if is_remote_include(path_str) {
        let url = if path_str.starts_with("github://") {
            resolve_github_uri(path_str)?
        } else {
            path_str.clone()
        };

        let content = fetch_remote(&url)?;

        // SHA-512 verification
        let computed_hash = compute_sha512(&content);

        match entry.sha512.as_deref() {
            Some(expected_hash) => {
                if computed_hash != expected_hash {
                    return Err(ConfigError::Include(format!(
                        "SHA-512 mismatch for '{}': expected {}, got {}",
                        path_str, expected_hash, computed_hash
                    )));
                }
                tracing::debug!(
                    include = path_str,
                    "remote include hash verified"
                );
            }
            None => {
                if security_mode == "strict" {
                    return Err(ConfigError::Include(format!(
                        "security_mode is 'strict' but include '{}' has no sha512 hash; \
                         add: sha512 = \"{}\"",
                        path_str, computed_hash
                    )));
                }
                // auto-pin: warn with the computed hash
                tracing::warn!(
                    include = path_str,
                    sha512 = computed_hash.as_str(),
                    "remote include loaded without hash — pin with: sha512 = \"{}\"",
                    computed_hash
                );
            }
        }

        return Ok(content);
    }

    // Local file include (relative or absolute path)
    let resolved = if Path::new(path_str).is_absolute() {
        PathBuf::from(path_str)
    } else {
        base_dir.join(path_str)
    };

    std::fs::read_to_string(&resolved).map_err(|e| {
        ConfigError::Include(format!(
            "failed to read include '{}' (resolved to '{}'): {}",
            path_str,
            resolved.display(),
            e
        ))
    })
}

/// Return the default config path in the user's home directory.
///
/// Returns `$HOME/.forge.toml` on Unix, `%USERPROFILE%\.forge.toml` on Windows.
pub fn home_config_path() -> Option<PathBuf> {
    #[cfg(unix)]
    let home = std::env::var("HOME").ok();
    #[cfg(windows)]
    let home = std::env::var("USERPROFILE")
        .ok()
        .or_else(|| std::env::var("HOME").ok());
    #[cfg(not(any(unix, windows)))]
    let home: Option<String> = None;

    home.map(|h| PathBuf::from(h).join(".forge.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parses_minimal_toml() {
        let toml = r#"
            [servers.narsil]
            command = "narsil-mcp"
            transport = "stdio"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.servers.len(), 1);
        let narsil = &config.servers["narsil"];
        assert_eq!(narsil.transport, "stdio");
        assert_eq!(narsil.command.as_deref(), Some("narsil-mcp"));
    }

    #[test]
    fn config_parses_sse_server() {
        let toml = r#"
            [servers.github]
            url = "https://mcp.github.com/sse"
            transport = "sse"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        let github = &config.servers["github"];
        assert_eq!(github.transport, "sse");
        assert_eq!(github.url.as_deref(), Some("https://mcp.github.com/sse"));
    }

    #[test]
    fn config_parses_sandbox_overrides() {
        let toml = r#"
            [sandbox]
            timeout_secs = 10
            max_heap_mb = 128
            max_concurrent = 4
            max_tool_calls = 100
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.sandbox.timeout_secs, Some(10));
        assert_eq!(config.sandbox.max_heap_mb, Some(128));
        assert_eq!(config.sandbox.max_concurrent, Some(4));
        assert_eq!(config.sandbox.max_tool_calls, Some(100));
    }

    #[test]
    fn config_expands_environment_variables() {
        temp_env::with_var("FORGE_TEST_TOKEN", Some("secret123"), || {
            let toml = r#"
                [servers.github]
                url = "https://mcp.github.com/sse"
                transport = "sse"
                headers = { Authorization = "Bearer ${FORGE_TEST_TOKEN}" }
            "#;

            let config = ForgeConfig::from_toml_with_env(toml).unwrap();
            let github = &config.servers["github"];
            assert_eq!(
                github.headers.get("Authorization").unwrap(),
                "Bearer secret123"
            );
        });
    }

    #[test]
    fn config_rejects_invalid_transport() {
        let toml = r#"
            [servers.test]
            command = "test"
            transport = "grpc"
        "#;

        let err = ForgeConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("grpc"),
            "error should mention the transport: {msg}"
        );
        assert!(
            msg.contains("stdio"),
            "error should mention supported transports: {msg}"
        );
    }

    #[test]
    fn config_rejects_stdio_without_command() {
        let toml = r#"
            [servers.test]
            transport = "stdio"
        "#;

        let err = ForgeConfig::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("command"));
    }

    #[test]
    fn config_rejects_sse_without_url() {
        let toml = r#"
            [servers.test]
            transport = "sse"
        "#;

        let err = ForgeConfig::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("url"));
    }

    #[test]
    fn config_loads_from_file() {
        let dir = std::env::temp_dir().join("forge-config-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
            [servers.test]
            command = "test-server"
            transport = "stdio"
        "#,
        )
        .unwrap();

        let config = ForgeConfig::from_file(&path).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert_eq!(
            config.servers["test"].command.as_deref(),
            Some("test-server")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_uses_defaults_when_absent() {
        let toml = r#"
            [servers.test]
            command = "test"
            transport = "stdio"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.sandbox.timeout_secs.is_none());
        assert!(config.sandbox.max_heap_mb.is_none());
        assert!(config.sandbox.max_concurrent.is_none());
        assert!(config.sandbox.max_tool_calls.is_none());
    }

    #[test]
    fn config_parses_full_example() {
        let toml = r#"
            [servers.narsil]
            command = "narsil-mcp"
            args = ["--repos", ".", "--streaming"]
            transport = "stdio"
            description = "Code intelligence"

            [servers.github]
            url = "https://mcp.github.com/sse"
            transport = "sse"
            headers = { Authorization = "Bearer token123" }

            [sandbox]
            timeout_secs = 5
            max_heap_mb = 64
            max_concurrent = 8
            max_tool_calls = 50
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.servers.len(), 2);

        let narsil = &config.servers["narsil"];
        assert_eq!(narsil.command.as_deref(), Some("narsil-mcp"));
        assert_eq!(narsil.args, vec!["--repos", ".", "--streaming"]);
        assert_eq!(narsil.description.as_deref(), Some("Code intelligence"));

        let github = &config.servers["github"];
        assert_eq!(github.url.as_deref(), Some("https://mcp.github.com/sse"));
        assert_eq!(
            github.headers.get("Authorization").unwrap(),
            "Bearer token123"
        );

        assert_eq!(config.sandbox.timeout_secs, Some(5));
    }

    #[test]
    fn config_empty_servers_is_valid() {
        let toml = "";
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.servers.is_empty());
    }

    #[test]
    fn env_var_expansion_preserves_unresolved() {
        let result = expand_env_vars("prefix ${DEFINITELY_NOT_SET_12345} suffix");
        assert_eq!(result, "prefix ${DEFINITELY_NOT_SET_12345} suffix");
    }

    #[test]
    fn env_var_expansion_handles_no_vars() {
        let result = expand_env_vars("no variables here");
        assert_eq!(result, "no variables here");
    }

    #[test]
    fn config_parses_execution_mode_child_process() {
        let toml = r#"
            [sandbox]
            execution_mode = "child_process"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(
            config.sandbox.execution_mode.as_deref(),
            Some("child_process")
        );
    }

    #[test]
    fn config_parses_groups() {
        let toml = r#"
            [servers.vault]
            command = "vault-mcp"
            transport = "stdio"

            [servers.slack]
            command = "slack-mcp"
            transport = "stdio"

            [groups.internal]
            servers = ["vault"]
            isolation = "strict"

            [groups.external]
            servers = ["slack"]
            isolation = "open"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.groups.len(), 2);
        assert_eq!(config.groups["internal"].isolation, "strict");
        assert_eq!(config.groups["external"].servers, vec!["slack"]);
    }

    #[test]
    fn config_groups_default_to_empty() {
        let toml = r#"
            [servers.test]
            command = "test"
            transport = "stdio"
        "#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.groups.is_empty());
    }

    #[test]
    fn config_rejects_group_with_unknown_server() {
        let toml = r#"
            [servers.real]
            command = "real"
            transport = "stdio"

            [groups.bad]
            servers = ["nonexistent"]
        "#;
        let err = ForgeConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("nonexistent"), "should mention server: {msg}");
        assert!(msg.contains("unknown"), "should say unknown: {msg}");
    }

    #[test]
    fn config_rejects_server_in_multiple_groups() {
        let toml = r#"
            [servers.shared]
            command = "shared"
            transport = "stdio"

            [groups.a]
            servers = ["shared"]

            [groups.b]
            servers = ["shared"]
        "#;
        let err = ForgeConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("shared"), "should mention server: {msg}");
        assert!(
            msg.contains("multiple groups"),
            "should say multiple groups: {msg}"
        );
    }

    #[test]
    fn config_rejects_invalid_isolation_mode() {
        let toml = r#"
            [servers.test]
            command = "test"
            transport = "stdio"

            [groups.bad]
            servers = ["test"]
            isolation = "paranoid"
        "#;
        let err = ForgeConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("paranoid"), "should mention mode: {msg}");
    }

    #[test]
    fn config_parses_server_timeout() {
        let toml = r#"
            [servers.slow]
            command = "slow-mcp"
            transport = "stdio"
            timeout_secs = 30
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.servers["slow"].timeout_secs, Some(30));
    }

    #[test]
    fn config_server_timeout_defaults_to_none() {
        let toml = r#"
            [servers.fast]
            command = "fast-mcp"
            transport = "stdio"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.servers["fast"].timeout_secs.is_none());
    }

    #[test]
    fn config_parses_circuit_breaker() {
        let toml = r#"
            [servers.flaky]
            command = "flaky-mcp"
            transport = "stdio"
            circuit_breaker = true
            failure_threshold = 5
            recovery_timeout_secs = 60
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        let flaky = &config.servers["flaky"];
        assert_eq!(flaky.circuit_breaker, Some(true));
        assert_eq!(flaky.failure_threshold, Some(5));
        assert_eq!(flaky.recovery_timeout_secs, Some(60));
    }

    #[test]
    fn config_circuit_breaker_defaults_to_none() {
        let toml = r#"
            [servers.stable]
            command = "stable-mcp"
            transport = "stdio"
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        let stable = &config.servers["stable"];
        assert!(stable.circuit_breaker.is_none());
        assert!(stable.failure_threshold.is_none());
        assert!(stable.recovery_timeout_secs.is_none());
    }

    #[test]
    fn config_execution_mode_defaults_to_none() {
        let toml = r#"
            [sandbox]
            timeout_secs = 5
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.sandbox.execution_mode.is_none());
    }

    // --- v0.2 Config Validation Tests (CV-01..CV-07) ---

    #[test]
    fn cv01_max_resource_size_mb_range() {
        // Valid (must fit within IPC limit — default 8 MB)
        let toml = "[sandbox]\nmax_resource_size_mb = 7";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Zero is invalid
        let toml = "[sandbox]\nmax_resource_size_mb = 0";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_resource_size_mb"), "got: {err}");

        // Over 512 is invalid
        let toml = "[sandbox]\nmax_resource_size_mb = 513";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_resource_size_mb"), "got: {err}");
    }

    #[test]
    fn cv02_max_parallel_range() {
        // Valid: within default max_concurrent (8)
        let toml = "[sandbox]\nmax_parallel = 4";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Zero is invalid
        let toml = "[sandbox]\nmax_parallel = 0";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_parallel"), "got: {err}");

        // Exceeding max_concurrent is invalid
        let toml = "[sandbox]\nmax_concurrent = 4\nmax_parallel = 5";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_parallel"), "got: {err}");
    }

    #[test]
    fn cv08_startup_concurrency_must_be_positive() {
        // Valid: any positive value
        let toml = "[sandbox]\nstartup_concurrency = 4";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        let toml = "[sandbox]\nstartup_concurrency = 1";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Zero is invalid
        let toml = "[sandbox]\nstartup_concurrency = 0";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(
            err.contains("startup_concurrency"),
            "expected 'startup_concurrency' in error: {err}"
        );
    }

    #[test]
    fn default_startup_concurrency_is_at_least_one() {
        let concurrency = default_startup_concurrency();
        assert!(
            concurrency >= 1,
            "default startup concurrency should be at least 1, got: {concurrency}"
        );
    }

    #[test]
    fn cv03_stash_max_value_size_mb_range() {
        // Valid
        let toml = "[sandbox.stash]\nmax_value_size_mb = 16";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Zero is invalid
        let toml = "[sandbox.stash]\nmax_value_size_mb = 0";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_value_size_mb"), "got: {err}");

        // Over 256 is invalid
        let toml = "[sandbox.stash]\nmax_value_size_mb = 257";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_value_size_mb"), "got: {err}");
    }

    #[test]
    fn cv04_stash_total_size_gte_value_size() {
        // Valid: total >= value
        let toml = "[sandbox.stash]\nmax_value_size_mb = 16\nmax_total_size_mb = 128";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Invalid: total < value
        let toml = "[sandbox.stash]\nmax_value_size_mb = 32\nmax_total_size_mb = 16";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_total_size_mb"), "got: {err}");
    }

    #[test]
    fn cv05_stash_default_ttl_range() {
        // Valid
        let toml = "[sandbox.stash]\ndefault_ttl_secs = 3600";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Zero is invalid
        let toml = "[sandbox.stash]\ndefault_ttl_secs = 0";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("default_ttl_secs"), "got: {err}");

        // Exceeding max_ttl is invalid
        let toml = "[sandbox.stash]\ndefault_ttl_secs = 100000\nmax_ttl_secs = 86400";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("default_ttl_secs"), "got: {err}");
    }

    #[test]
    fn cv06_stash_max_ttl_range() {
        // Valid
        let toml = "[sandbox.stash]\nmax_ttl_secs = 86400";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Zero is invalid
        let toml = "[sandbox.stash]\nmax_ttl_secs = 0";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_ttl_secs"), "got: {err}");

        // Over 7 days is invalid
        let toml = "[sandbox.stash]\nmax_ttl_secs = 604801";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("max_ttl_secs"), "got: {err}");
    }

    #[test]
    fn cv07_max_resource_size_fits_ipc() {
        // Valid: 7 MB + 1 MB overhead = 8 MB = fits default IPC limit
        let toml = "[sandbox]\nmax_resource_size_mb = 7";
        assert!(ForgeConfig::from_toml(toml).is_ok());

        // Invalid: 8 MB + 1 MB overhead = 9 MB > 8 MB default IPC limit
        let toml = "[sandbox]\nmax_resource_size_mb = 8";
        let err = ForgeConfig::from_toml(toml).unwrap_err().to_string();
        assert!(err.contains("IPC"), "got: {err}");

        // Valid with explicit larger IPC limit
        let toml = "[sandbox]\nmax_resource_size_mb = 32\nmax_ipc_message_size_mb = 64";
        assert!(ForgeConfig::from_toml(toml).is_ok());
    }

    #[test]
    fn config_parses_v02_sandbox_fields() {
        let toml = r#"
            [sandbox]
            max_resource_size_mb = 7
            max_ipc_message_size_mb = 64
            max_parallel = 4

            [sandbox.stash]
            max_keys = 128
            max_value_size_mb = 8
            max_total_size_mb = 64
            default_ttl_secs = 1800
            max_ttl_secs = 43200
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.sandbox.max_resource_size_mb, Some(7));
        assert_eq!(config.sandbox.max_ipc_message_size_mb, Some(64));
        assert_eq!(config.sandbox.max_parallel, Some(4));

        let stash = config.sandbox.stash.unwrap();
        assert_eq!(stash.max_keys, Some(128));
        assert_eq!(stash.max_value_size_mb, Some(8));
        assert_eq!(stash.max_total_size_mb, Some(64));
        assert_eq!(stash.default_ttl_secs, Some(1800));
        assert_eq!(stash.max_ttl_secs, Some(43200));
    }

    // --- Pool config tests (CV-08 to CV-11) ---

    #[test]
    fn cv_08_pool_max_workers_validation() {
        // max_workers = 0 is invalid
        let toml = r#"
            [sandbox.pool]
            enabled = true
            max_workers = 0
        "#;
        assert!(ForgeConfig::from_toml(toml).is_err());

        // max_workers > max_concurrent is invalid
        let toml = r#"
            [sandbox]
            max_concurrent = 4
            [sandbox.pool]
            max_workers = 5
        "#;
        assert!(ForgeConfig::from_toml(toml).is_err());

        // max_workers within range is valid
        let toml = r#"
            [sandbox]
            max_concurrent = 8
            [sandbox.pool]
            max_workers = 4
        "#;
        assert!(ForgeConfig::from_toml(toml).is_ok());
    }

    #[test]
    fn cv_09_pool_min_workers_validation() {
        // min_workers > max_workers is invalid
        let toml = r#"
            [sandbox.pool]
            min_workers = 5
            max_workers = 2
        "#;
        assert!(ForgeConfig::from_toml(toml).is_err());

        // min_workers <= max_workers is valid
        let toml = r#"
            [sandbox.pool]
            min_workers = 2
            max_workers = 4
        "#;
        assert!(ForgeConfig::from_toml(toml).is_ok());
    }

    #[test]
    fn cv_10_pool_max_uses_validation() {
        // max_uses = 0 is invalid
        let toml = r#"
            [sandbox.pool]
            max_uses = 0
        "#;
        assert!(ForgeConfig::from_toml(toml).is_err());

        // max_uses > 0 is valid
        let toml = r#"
            [sandbox.pool]
            max_uses = 100
        "#;
        assert!(ForgeConfig::from_toml(toml).is_ok());
    }

    #[test]
    fn cv_11_pool_max_idle_validation() {
        // max_idle_secs < 5 is invalid
        let toml = r#"
            [sandbox.pool]
            max_idle_secs = 2
        "#;
        assert!(ForgeConfig::from_toml(toml).is_err());

        // max_idle_secs > 3600 is invalid
        let toml = r#"
            [sandbox.pool]
            max_idle_secs = 7200
        "#;
        assert!(ForgeConfig::from_toml(toml).is_err());

        // max_idle_secs = 60 is valid
        let toml = r#"
            [sandbox.pool]
            max_idle_secs = 60
        "#;
        assert!(ForgeConfig::from_toml(toml).is_ok());
    }

    #[test]
    fn config_parses_pool_fields() {
        let toml = r#"
            [sandbox.pool]
            enabled = true
            min_workers = 2
            max_workers = 8
            max_idle_secs = 60
            max_uses = 50
        "#;

        let config = ForgeConfig::from_toml(toml).unwrap();
        let pool = config.sandbox.pool.unwrap();
        assert_eq!(pool.enabled, Some(true));
        assert_eq!(pool.min_workers, Some(2));
        assert_eq!(pool.max_workers, Some(8));
        assert_eq!(pool.max_idle_secs, Some(60));
        assert_eq!(pool.max_uses, Some(50));
    }

    // --- Production config parse tests (CFG-P01..CFG-P06) ---

    fn load_production_example() -> ForgeConfig {
        let toml_str = include_str!("../../../forge.toml.example.production");
        ForgeConfig::from_toml(toml_str).expect("production example must parse")
    }

    #[test]
    fn cfg_p01_production_example_parses() {
        let config = load_production_example();
        assert!(!config.servers.is_empty(), "should have servers");
    }

    #[test]
    fn cfg_p02_production_pool_enabled() {
        let config = load_production_example();
        let pool = config.sandbox.pool.as_ref().expect("pool section required");
        assert_eq!(pool.enabled, Some(true));
        assert!(pool.min_workers.is_some());
        assert!(pool.max_workers.is_some());
    }

    #[test]
    fn cfg_p03_production_strict_groups() {
        let config = load_production_example();
        assert!(!config.groups.is_empty(), "should have groups");
        let has_strict = config.groups.values().any(|g| g.isolation == "strict");
        assert!(has_strict, "should have at least one strict group");
    }

    #[test]
    fn cfg_p04_production_stash_configured() {
        let config = load_production_example();
        let stash = config
            .sandbox
            .stash
            .as_ref()
            .expect("stash section required");
        assert!(stash.max_keys.is_some());
        assert!(stash.max_total_size_mb.is_some());
    }

    #[test]
    fn cfg_p05_production_circuit_breakers() {
        let config = load_production_example();
        for (name, server) in &config.servers {
            assert_eq!(
                server.circuit_breaker,
                Some(true),
                "server '{}' should have circuit_breaker = true",
                name
            );
        }
    }

    #[test]
    fn cfg_p06_production_execution_mode_child_process() {
        let config = load_production_example();
        assert_eq!(
            config.sandbox.execution_mode.as_deref(),
            Some("child_process")
        );
    }

    /// Verify that `config-watch` feature is on by default (v0.4.0+).
    #[test]
    #[cfg(feature = "config-watch")]
    fn ff_d03_config_watch_is_default() {
        // config-watch is default-on since v0.4.0.
        // Verify the watcher module type is accessible.
        let _ = std::any::type_name::<crate::watcher::ConfigWatcher>();
    }

    // --- Upgrade path compatibility tests (UP-01..UP-03) ---

    #[test]
    fn up_01_v03x_config_without_pool_section() {
        // v0.3.x configs may not have [sandbox.pool] at all
        let toml = r#"
            [servers.test]
            command = "test-mcp"
            transport = "stdio"

            [sandbox]
            timeout_secs = 5
        "#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.sandbox.pool.is_none());
    }

    #[test]
    fn up_02_v03x_config_without_manifest_section() {
        // v0.3.x configs may not have [manifest] at all
        let toml = r#"
            [servers.test]
            command = "test-mcp"
            transport = "stdio"
        "#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.manifest.refresh_interval_secs.is_none());
    }

    #[test]
    fn up_03_v03x_config_without_groups_or_stash() {
        // v0.3.x minimal config: just servers and sandbox basics
        let toml = r#"
            [servers.narsil]
            command = "narsil-mcp"
            args = ["--repos", "."]
            transport = "stdio"

            [sandbox]
            timeout_secs = 5
            max_heap_mb = 64
            max_concurrent = 8
            max_tool_calls = 50
            execution_mode = "child_process"
        "#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.groups.is_empty());
        assert!(config.sandbox.stash.is_none());
        assert!(config.sandbox.pool.is_none());
        assert_eq!(config.servers.len(), 1);
    }

    /// Compile-time guard: ConfigError is #[non_exhaustive].
    #[test]
    #[allow(unreachable_patterns)]
    fn ne_config_error_is_non_exhaustive() {
        let err = ConfigError::Invalid("test".into());
        match err {
            ConfigError::Invalid(_) | ConfigError::Parse(_) => {}
            _ => {}
        }
    }

    // --- Include and security_mode tests ---

    #[test]
    fn config_defaults_security_mode_auto_pin() {
        let toml = "";
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.security_mode, "auto-pin");
    }

    #[test]
    fn config_parses_security_mode_strict() {
        let toml = r#"security_mode = "strict""#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.security_mode, "strict");
    }

    #[test]
    fn config_rejects_invalid_security_mode() {
        let toml = r#"security_mode = "yolo""#;
        let err = ForgeConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("yolo"), "should mention mode: {msg}");
        assert!(
            msg.contains("security_mode"),
            "should mention field: {msg}"
        );
    }

    #[test]
    fn config_defaults_include_empty() {
        let toml = "";
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert!(config.include.is_empty());
    }

    #[test]
    fn config_parses_include_entries() {
        let toml = r#"
            [[include]]
            path = "./shared.toml"

            [[include]]
            path = "https://example.com/config.toml"
            sha512 = "abc123"
        "#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.include.len(), 2);
        assert_eq!(config.include[0].path, "./shared.toml");
        assert!(config.include[0].sha512.is_none());
        assert_eq!(config.include[1].path, "https://example.com/config.toml");
        assert_eq!(config.include[1].sha512.as_deref(), Some("abc123"));
    }

    #[test]
    fn config_local_include_merges_servers() {
        let dir = std::env::temp_dir().join("forge-config-test-include");
        std::fs::create_dir_all(&dir).unwrap();

        // Create included config file
        let included_path = dir.join("shared.toml");
        std::fs::write(
            &included_path,
            r#"
[servers.shared_server]
command = "shared-mcp"
transport = "stdio"
"#,
        )
        .unwrap();

        // Create main config file
        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            r#"
[[include]]
path = "./shared.toml"

[servers.local_server]
command = "local-mcp"
transport = "stdio"
"#,
        )
        .unwrap();

        let config = ForgeConfig::from_file(&main_path).unwrap();
        assert_eq!(config.servers.len(), 2);
        assert!(config.servers.contains_key("local_server"));
        assert!(config.servers.contains_key("shared_server"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_local_include_main_wins_on_conflict() {
        let dir = std::env::temp_dir().join("forge-config-test-include-conflict");
        std::fs::create_dir_all(&dir).unwrap();

        // Create included config with server "x"
        let included_path = dir.join("shared.toml");
        std::fs::write(
            &included_path,
            r#"
[servers.x]
command = "from-include"
transport = "stdio"
"#,
        )
        .unwrap();

        // Create main config with the same server "x"
        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            r#"
[[include]]
path = "./shared.toml"

[servers.x]
command = "from-main"
transport = "stdio"
"#,
        )
        .unwrap();

        let config = ForgeConfig::from_file(&main_path).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert_eq!(
            config.servers["x"].command.as_deref(),
            Some("from-main"),
            "main config should win on conflict"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_local_include_absolute_path() {
        let dir = std::env::temp_dir().join("forge-config-test-include-abs");
        std::fs::create_dir_all(&dir).unwrap();

        let included_path = dir.join("abs-include.toml");
        std::fs::write(
            &included_path,
            r#"
[servers.abs_server]
command = "abs-mcp"
transport = "stdio"
"#,
        )
        .unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            format!(
                r#"
[[include]]
path = "{}"
"#,
                included_path.display()
            ),
        )
        .unwrap();

        let config = ForgeConfig::from_file(&main_path).unwrap();
        assert!(config.servers.contains_key("abs_server"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_local_include_file_uri() {
        let dir = std::env::temp_dir().join("forge-config-test-include-file-uri");
        std::fs::create_dir_all(&dir).unwrap();

        let included_path = dir.join("file-include.toml");
        std::fs::write(
            &included_path,
            r#"
[servers.file_server]
command = "file-mcp"
transport = "stdio"
"#,
        )
        .unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            format!(
                r#"
[[include]]
path = "file://{}"
"#,
                included_path.display()
            ),
        )
        .unwrap();

        let config = ForgeConfig::from_file(&main_path).unwrap();
        assert!(config.servers.contains_key("file_server"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_include_missing_file_errors() {
        let dir = std::env::temp_dir().join("forge-config-test-include-missing");
        std::fs::create_dir_all(&dir).unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            r#"
[[include]]
path = "./does-not-exist.toml"
"#,
        )
        .unwrap();

        let err = ForgeConfig::from_file(&main_path).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("does-not-exist.toml"),
            "should mention missing file: {msg}"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_include_merges_groups() {
        let dir = std::env::temp_dir().join("forge-config-test-include-groups");
        std::fs::create_dir_all(&dir).unwrap();

        let included_path = dir.join("shared.toml");
        std::fs::write(
            &included_path,
            r#"
[servers.inc_server]
command = "inc-mcp"
transport = "stdio"

[groups.inc_group]
servers = ["inc_server"]
isolation = "open"
"#,
        )
        .unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            r#"
[[include]]
path = "./shared.toml"

[servers.main_server]
command = "main-mcp"
transport = "stdio"

[groups.main_group]
servers = ["main_server"]
isolation = "strict"
"#,
        )
        .unwrap();

        let config = ForgeConfig::from_file(&main_path).unwrap();
        assert_eq!(config.servers.len(), 2);
        assert_eq!(config.groups.len(), 2);
        assert!(config.groups.contains_key("inc_group"));
        assert!(config.groups.contains_key("main_group"));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_include_env_expansion_in_included_file() {
        let dir = std::env::temp_dir().join("forge-config-test-include-env");
        std::fs::create_dir_all(&dir).unwrap();

        let included_path = dir.join("env-include.toml");
        std::fs::write(
            &included_path,
            r#"
[servers.env_server]
url = "https://example.com/${FORGE_INCLUDE_TEST_TOKEN}"
transport = "sse"
"#,
        )
        .unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            r#"
[[include]]
path = "./env-include.toml"
"#,
        )
        .unwrap();

        temp_env::with_var("FORGE_INCLUDE_TEST_TOKEN", Some("secret_val"), || {
            let config = ForgeConfig::from_file_with_env(&main_path).unwrap();
            assert_eq!(
                config.servers["env_server"].url.as_deref(),
                Some("https://example.com/secret_val")
            );
        });

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_remote_include_identifies_remotes() {
        assert!(is_remote_include("https://example.com/config.toml"));
        assert!(is_remote_include("http://example.com/config.toml"));
        assert!(is_remote_include("github://org/repo/file.toml"));
        assert!(!is_remote_include("./local.toml"));
        assert!(!is_remote_include("/absolute/path.toml"));
        assert!(!is_remote_include("file:///path/to/file.toml"));
    }

    #[test]
    fn resolve_github_uri_default_branch() {
        let url =
            resolve_github_uri("github://myorg/myrepo/configs/forge.toml").unwrap();
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/myorg/myrepo/main/configs/forge.toml"
        );
    }

    #[test]
    fn resolve_github_uri_with_ref() {
        let url =
            resolve_github_uri("github://myorg/myrepo@v1.0/configs/forge.toml").unwrap();
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/myorg/myrepo/v1.0/configs/forge.toml"
        );
    }

    #[test]
    fn resolve_github_uri_with_sha_ref() {
        let url =
            resolve_github_uri("github://myorg/myrepo@abc123/path/file.toml").unwrap();
        assert_eq!(
            url,
            "https://raw.githubusercontent.com/myorg/myrepo/abc123/path/file.toml"
        );
    }

    #[test]
    fn resolve_github_uri_invalid_format() {
        // Missing path component
        let err = resolve_github_uri("github://myorg/myrepo").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("github://"), "should mention format: {msg}");
    }

    #[test]
    fn compute_sha512_deterministic() {
        let hash1 = compute_sha512("hello world");
        let hash2 = compute_sha512("hello world");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 128); // SHA-512 = 64 bytes = 128 hex chars
    }

    #[test]
    fn compute_sha512_different_input() {
        let hash1 = compute_sha512("hello");
        let hash2 = compute_sha512("world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn home_config_path_returns_some() {
        // On Unix CI, HOME is typically set
        #[cfg(unix)]
        {
            if std::env::var("HOME").is_ok() {
                let path = home_config_path();
                assert!(path.is_some());
                let p = path.unwrap();
                assert!(p.to_str().unwrap().ends_with(".forge.toml"));
            }
        }
    }

    #[test]
    fn config_backward_compat_no_include_no_security_mode() {
        // Old configs without include/security_mode should still parse
        let toml = r#"
            [servers.test]
            command = "test"
            transport = "stdio"

            [sandbox]
            timeout_secs = 5

            [groups.g]
            servers = ["test"]
        "#;
        let config = ForgeConfig::from_toml(toml).unwrap();
        assert_eq!(config.security_mode, "auto-pin");
        assert!(config.include.is_empty());
        assert_eq!(config.servers.len(), 1);
    }

    #[test]
    fn config_nested_includes_not_followed() {
        // Verify that includes in included files are not processed
        let dir = std::env::temp_dir().join("forge-config-test-nested-include");
        std::fs::create_dir_all(&dir).unwrap();

        // Create a "grandchild" include (should NOT be loaded)
        let grandchild_path = dir.join("grandchild.toml");
        std::fs::write(
            &grandchild_path,
            r#"
[servers.grandchild_server]
command = "grandchild"
transport = "stdio"
"#,
        )
        .unwrap();

        // Create "child" include that itself has an include
        let child_path = dir.join("child.toml");
        std::fs::write(
            &child_path,
            r#"
[[include]]
path = "./grandchild.toml"

[servers.child_server]
command = "child"
transport = "stdio"
"#,
        )
        .unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            r#"
[[include]]
path = "./child.toml"
"#,
        )
        .unwrap();

        let config = ForgeConfig::from_file(&main_path).unwrap();
        // child_server should be present
        assert!(config.servers.contains_key("child_server"));
        // grandchild_server should NOT be present (nested includes not followed)
        assert!(
            !config.servers.contains_key("grandchild_server"),
            "nested includes should not be followed"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
