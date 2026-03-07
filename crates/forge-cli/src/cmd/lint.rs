//! `forgemax lint` — validate configuration without running servers.
//!
//! This command checks that the configuration file is syntactically valid,
//! environment variables resolve, permissions are correct, and all references
//! (groups, includes) are consistent.
//!
//! Shared check types and config validation functions are defined here and
//! reused by the `doctor` command.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::common;

/// Arguments for the lint subcommand.
#[derive(Debug, Args)]
pub struct LintArgs {
    /// Output results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Overall status for a single check.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CheckStatus {
    /// Check passed.
    Pass,
    /// Check produced a warning (non-fatal).
    Warn,
    /// Check failed.
    Fail,
}

impl std::fmt::Display for CheckStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckStatus::Pass => write!(f, "PASS"),
            CheckStatus::Warn => write!(f, "WARN"),
            CheckStatus::Fail => write!(f, "FAIL"),
        }
    }
}

/// A single check result.
#[derive(Debug, Clone, Serialize)]
pub struct Check {
    /// Check name (e.g., "config_valid").
    pub name: String,
    /// Check result.
    pub status: CheckStatus,
    /// Human-readable description.
    pub message: String,
    /// Suggested fix, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

/// The lint/doctor report.
#[derive(Debug, Serialize)]
pub struct LintReport {
    /// Schema version for JSON output stability.
    pub schema_version: u32,
    /// Whether all checks passed (no failures).
    pub passed: bool,
    /// Individual check results.
    pub checks: Vec<Check>,
    /// Summary message.
    pub summary: String,
}

// ──────────────────────────────────────────────────────────────────────
// Shared config validation checks (used by both lint and doctor)
// ──────────────────────────────────────────────────────────────────────

/// Check that the config file parses and validates.
pub fn check_config_valid(config_path: Option<&PathBuf>) -> Check {
    let path = config_path.cloned().or_else(common::find_config_file);
    match path {
        Some(ref p) => match forge_config::ForgeConfig::from_file_with_env(p) {
            Ok(config) => {
                let include_note = if config.include.is_empty() {
                    String::new()
                } else {
                    format!(", {} include(s) loaded", config.include.len())
                };
                Check {
                    name: "config_valid".into(),
                    status: CheckStatus::Pass,
                    message: format!(
                        "config file parses: {}{}",
                        p.display(),
                        include_note
                    ),
                    fix: None,
                }
            }
            Err(e) => Check {
                name: "config_valid".into(),
                status: CheckStatus::Fail,
                message: format!("config parse error: {}", e),
                fix: Some("Fix the configuration file syntax".into()),
            },
        },
        None => Check {
            name: "config_valid".into(),
            status: CheckStatus::Warn,
            message: "no config file found".into(),
            fix: Some(
                "Create forge.toml or set FORGE_CONFIG env var. Run `forgemax init` to generate one."
                    .into(),
            ),
        },
    }
}

/// Check that environment variable references in the config resolve.
/// Scans both the main config file and any local included files.
pub fn check_env_vars(config_path: Option<&PathBuf>) -> Check {
    let path = config_path.cloned().or_else(common::find_config_file);
    match path {
        Some(ref p) => match std::fs::read_to_string(p) {
            Ok(content) => {
                let mut all_vars = common::find_env_var_refs(&content);

                // Also scan local included files for env var references
                if let Ok(config) = forge_config::ForgeConfig::from_toml(&content) {
                    let base_dir = p.parent().unwrap_or_else(|| std::path::Path::new("."));
                    for entry in &config.include {
                        if let Some(inc_content) = resolve_include_content(entry, base_dir) {
                            all_vars.extend(common::find_env_var_refs(&inc_content));
                        }
                    }
                }

                let mut missing = Vec::new();
                for var in &all_vars {
                    if std::env::var(var).is_err() {
                        missing.push(var.clone());
                    }
                }
                // Deduplicate missing vars
                missing.sort();
                missing.dedup();

                if missing.is_empty() {
                    Check {
                        name: "env_vars".into(),
                        status: CheckStatus::Pass,
                        message: if all_vars.is_empty() {
                            "no environment variable references found".into()
                        } else {
                            format!("all {} env var references resolve", all_vars.len())
                        },
                        fix: None,
                    }
                } else {
                    Check {
                        name: "env_vars".into(),
                        status: CheckStatus::Fail,
                        message: format!("unresolved env vars: {}", missing.join(", ")),
                        fix: Some(
                            "Set the missing environment variables before starting".into(),
                        ),
                    }
                }
            }
            Err(e) => Check {
                name: "env_vars".into(),
                status: CheckStatus::Fail,
                message: format!("cannot read config file: {}", e),
                fix: None,
            },
        },
        None => Check {
            name: "env_vars".into(),
            status: CheckStatus::Pass,
            message: "no config file to check".into(),
            fix: None,
        },
    }
}

/// Try to read the content of a local include entry for env var scanning.
/// Returns None for remote includes or on read errors.
fn resolve_include_content(
    entry: &forge_config::IncludeEntry,
    base_dir: &std::path::Path,
) -> Option<String> {
    let path_str = &entry.path;

    // Skip remote includes — only scan local files
    if forge_config::is_remote_include(path_str) {
        return None;
    }

    let resolved = if let Some(file_path) = path_str.strip_prefix("file://") {
        std::path::PathBuf::from(file_path)
    } else if std::path::Path::new(path_str).is_absolute() {
        std::path::PathBuf::from(path_str)
    } else {
        base_dir.join(path_str)
    };

    std::fs::read_to_string(&resolved).ok()
}

/// Check file permissions on the config file (Unix only).
#[cfg(unix)]
pub fn check_config_permissions(config_path: Option<&PathBuf>) -> Check {
    use std::os::unix::fs::PermissionsExt;

    let path = config_path.cloned().or_else(common::find_config_file);
    match path {
        Some(ref p) => match std::fs::metadata(p) {
            Ok(meta) => {
                let mode = meta.permissions().mode();
                let has_secrets = std::fs::read_to_string(p)
                    .map(|c| c.contains("${"))
                    .unwrap_or(false);
                if has_secrets && (mode & 0o044) != 0 {
                    Check {
                        name: "config_permissions".into(),
                        status: CheckStatus::Warn,
                        message: format!(
                            "config with secrets is group/world-readable (mode: {:o})",
                            mode & 0o777
                        ),
                        fix: Some(format!("chmod 600 {}", p.display())),
                    }
                } else {
                    Check {
                        name: "config_permissions".into(),
                        status: CheckStatus::Pass,
                        message: format!(
                            "config permissions OK (mode: {:o})",
                            mode & 0o777
                        ),
                        fix: None,
                    }
                }
            }
            Err(e) => Check {
                name: "config_permissions".into(),
                status: CheckStatus::Warn,
                message: format!("cannot stat config file: {}", e),
                fix: None,
            },
        },
        None => Check {
            name: "config_permissions".into(),
            status: CheckStatus::Pass,
            message: "no config file to check".into(),
            fix: None,
        },
    }
}

/// Check file permissions (non-Unix stub).
#[cfg(not(unix))]
pub fn check_config_permissions(_config_path: Option<&PathBuf>) -> Check {
    Check {
        name: "config_permissions".into(),
        status: CheckStatus::Pass,
        message: "permission check skipped (non-Unix platform)".into(),
        fix: None,
    }
}

/// Check that group definitions reference valid servers and find orphans.
pub fn check_groups(config_path: Option<&PathBuf>) -> Check {
    let config = match common::load_config(config_path) {
        Ok(c) => c,
        Err(_) => {
            return Check {
                name: "groups".into(),
                status: CheckStatus::Pass,
                message: "no valid config to check groups".into(),
                fix: None,
            };
        }
    };

    if config.groups.is_empty() {
        return Check {
            name: "groups".into(),
            status: CheckStatus::Pass,
            message: "no groups configured".into(),
            fix: None,
        };
    }

    let server_names: std::collections::HashSet<&str> =
        config.servers.keys().map(|s| s.as_str()).collect();
    let grouped_servers: std::collections::HashSet<&str> = config
        .groups
        .values()
        .flat_map(|g| g.servers.iter().map(|s| s.as_str()))
        .collect();

    let mut orphaned: Vec<&str> = server_names.difference(&grouped_servers).copied().collect();
    orphaned.sort();

    if orphaned.is_empty() {
        Check {
            name: "groups".into(),
            status: CheckStatus::Pass,
            message: format!(
                "{} group(s) covering all {} server(s)",
                config.groups.len(),
                server_names.len()
            ),
            fix: None,
        }
    } else {
        Check {
            name: "groups".into(),
            status: CheckStatus::Warn,
            message: format!("servers not in any group: {}", orphaned.join(", ")),
            fix: Some(
                "Add ungrouped servers to a group or leave ungrouped if intentional".into(),
            ),
        }
    }
}

/// Check that HTTP/SSE servers have circuit breakers configured.
pub fn check_circuit_breakers(config_path: Option<&PathBuf>) -> Check {
    let config = match common::load_config(config_path) {
        Ok(c) => c,
        Err(_) => {
            return Check {
                name: "circuit_breakers".into(),
                status: CheckStatus::Pass,
                message: "no valid config to check".into(),
                fix: None,
            };
        }
    };

    let unprotected: Vec<&str> = config
        .servers
        .iter()
        .filter(|(_, s)| s.transport == "sse" && s.circuit_breaker != Some(true))
        .map(|(name, _)| name.as_str())
        .collect();

    if unprotected.is_empty() {
        Check {
            name: "circuit_breakers".into(),
            status: CheckStatus::Pass,
            message: "all SSE servers have circuit breakers configured".into(),
            fix: None,
        }
    } else {
        Check {
            name: "circuit_breakers".into(),
            status: CheckStatus::Warn,
            message: format!(
                "SSE servers without circuit breakers: {}",
                unprotected.join(", ")
            ),
            fix: Some(format!(
                "Add circuit_breaker = true to: {}",
                unprotected.join(", ")
            )),
        }
    }
}

/// Check that token/header values don't contain common formatting mistakes.
pub fn check_token_formats(config_path: Option<&PathBuf>) -> Check {
    let config = match common::load_config(config_path) {
        Ok(c) => c,
        Err(_) => {
            return Check {
                name: "token_formats".into(),
                status: CheckStatus::Pass,
                message: "no valid config to check".into(),
                fix: None,
            };
        }
    };

    let mut issues = Vec::new();

    for (name, server) in &config.servers {
        for (key, value) in &server.headers {
            if value.starts_with('"') || value.ends_with('"') {
                issues.push(format!("{name}: {key} has embedded quotes"));
            }
            if value.contains('\n') || value.contains('\r') {
                issues.push(format!("{name}: {key} contains newlines"));
            }
            if value.starts_with("Bearer ") && key.to_lowercase() != "authorization" {
                issues.push(format!(
                    "{name}: {key} has 'Bearer ' prefix (should be bare token)"
                ));
            }
        }
    }

    if issues.is_empty() {
        Check {
            name: "token_formats".into(),
            status: CheckStatus::Pass,
            message: "all header values look well-formed".into(),
            fix: None,
        }
    } else {
        Check {
            name: "token_formats".into(),
            status: CheckStatus::Warn,
            message: format!("potential token issues: {}", issues.join("; ")),
            fix: Some(
                "Check header values — remove quotes, newlines, and misplaced 'Bearer ' prefixes"
                    .into(),
            ),
        }
    }
}

/// Check the security_mode setting.
pub fn check_security_mode(config_path: Option<&PathBuf>) -> Check {
    let config = match common::load_config(config_path) {
        Ok(c) => c,
        Err(_) => {
            return Check {
                name: "security_mode".into(),
                status: CheckStatus::Pass,
                message: "no valid config to check".into(),
                fix: None,
            };
        }
    };

    let has_remote_includes = config
        .include
        .iter()
        .any(|i| forge_config::is_remote_include(&i.path));

    if !has_remote_includes {
        return Check {
            name: "security_mode".into(),
            status: CheckStatus::Pass,
            message: format!(
                "security_mode = '{}' (no remote includes)",
                config.security_mode
            ),
            fix: None,
        };
    }

    let unpinned: Vec<&str> = config
        .include
        .iter()
        .filter(|i| {
            forge_config::is_remote_include(&i.path) && i.sha512.is_none()
        })
        .map(|i| i.path.as_str())
        .collect();

    if config.security_mode == "strict" && !unpinned.is_empty() {
        Check {
            name: "security_mode".into(),
            status: CheckStatus::Fail,
            message: format!(
                "security_mode = 'strict' but {} remote include(s) lack sha512 hash: {}",
                unpinned.len(),
                unpinned.join(", ")
            ),
            fix: Some("Add sha512 hashes to all remote includes".into()),
        }
    } else if !unpinned.is_empty() {
        Check {
            name: "security_mode".into(),
            status: CheckStatus::Warn,
            message: format!(
                "security_mode = 'auto-pin': {} remote include(s) not pinned",
                unpinned.len()
            ),
            fix: Some("Run `forgemax lint` and add the logged sha512 hashes".into()),
        }
    } else {
        Check {
            name: "security_mode".into(),
            status: CheckStatus::Pass,
            message: format!(
                "security_mode = '{}', all remote includes pinned",
                config.security_mode
            ),
            fix: None,
        }
    }
}

/// Run all config-only validation checks. Used by both `lint` and `doctor`.
pub fn run_config_checks(config_path: Option<&PathBuf>) -> Vec<Check> {
    vec![
        check_config_permissions(config_path),
        check_config_valid(config_path),
        check_env_vars(config_path),
        check_groups(config_path),
        check_circuit_breakers(config_path),
        check_token_formats(config_path),
        check_security_mode(config_path),
    ]
}

/// Build a summary string from a set of checks.
pub fn build_summary(checks: &[Check]) -> (bool, String) {
    let pass_count = checks.iter().filter(|c| c.status == CheckStatus::Pass).count();
    let warn_count = checks.iter().filter(|c| c.status == CheckStatus::Warn).count();
    let fail_count = checks.iter().filter(|c| c.status == CheckStatus::Fail).count();
    let has_fail = fail_count > 0;
    let summary = format!(
        "{} passed, {} warnings, {} failed",
        pass_count, warn_count, fail_count
    );
    (has_fail, summary)
}

/// Print checks in human-readable format with ANSI colors.
pub fn print_checks(checks: &[Check], summary: &str) {
    for check in checks {
        let status_str = match check.status {
            CheckStatus::Pass => "\x1b[32mPASS\x1b[0m",
            CheckStatus::Warn => "\x1b[33mWARN\x1b[0m",
            CheckStatus::Fail => "\x1b[31mFAIL\x1b[0m",
        };
        println!("  [{}] {}: {}", status_str, check.name, check.message);
        if let Some(ref fix) = check.fix {
            println!("         fix: {}", fix);
        }
    }
    println!();
    println!("  {}", summary);
}

/// Execute the lint command.
pub async fn execute(args: &LintArgs, config_path: Option<PathBuf>) -> Result<()> {
    let config_ref = config_path.as_ref();
    let checks = run_config_checks(config_ref);

    let (has_fail, summary) = build_summary(&checks);

    let report = LintReport {
        schema_version: 1,
        passed: !has_fail,
        checks,
        summary: summary.clone(),
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_checks(&report.checks, &summary);
    }

    if has_fail {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lint_01_check_config_valid_with_valid_toml() {
        let dir = std::env::temp_dir().join("forge-lint-test-valid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\n",
        )
        .unwrap();
        let check = check_config_valid(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_02_check_config_valid_missing() {
        let path = PathBuf::from("/nonexistent/forge.toml");
        let check = check_config_valid(Some(&path));
        assert_eq!(check.status, CheckStatus::Fail);
    }

    #[test]
    fn lint_03_check_config_valid_invalid_toml() {
        let dir = std::env::temp_dir().join("forge-lint-test-invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "[[[invalid toml").unwrap();
        let check = check_config_valid(Some(&path));
        assert_eq!(check.status, CheckStatus::Fail);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_04_check_env_vars_all_set() {
        let dir = std::env::temp_dir().join("forge-lint-test-env");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "# no env var refs\n[sandbox]\ntimeout_secs = 5\n").unwrap();
        let check = check_env_vars(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_05_check_env_vars_unresolved() {
        let dir = std::env::temp_dir().join("forge-lint-test-env-missing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\nheaders = { Auth = \"${FORGE_LINT_TEST_NONEXISTENT_VAR}\" }\n",
        )
        .unwrap();
        let check = check_env_vars(Some(&path));
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("FORGE_LINT_TEST_NONEXISTENT_VAR"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_06_check_groups_no_config() {
        let path = PathBuf::from("/nonexistent/forge.toml");
        let check = check_groups(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn lint_07_check_groups_orphaned_servers() {
        let dir = std::env::temp_dir().join("forge-lint-test-groups");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.a]
command = "a"
transport = "stdio"
[servers.b]
command = "b"
transport = "stdio"
[groups.grp]
servers = ["a"]
"#,
        )
        .unwrap();
        let check = check_groups(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(
            check.message.contains("b"),
            "should mention orphaned server"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_08_report_json_valid() {
        let report = LintReport {
            schema_version: 1,
            passed: true,
            checks: vec![Check {
                name: "test".into(),
                status: CheckStatus::Pass,
                message: "ok".into(),
                fix: None,
            }],
            summary: "1 passed, 0 warnings, 0 failed".into(),
        };
        let json = serde_json::to_string_pretty(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["schema_version"], 1);
        assert_eq!(parsed["passed"], true);
    }

    #[test]
    fn lint_09_build_summary() {
        let checks = vec![
            Check {
                name: "a".into(),
                status: CheckStatus::Pass,
                message: "ok".into(),
                fix: None,
            },
            Check {
                name: "b".into(),
                status: CheckStatus::Warn,
                message: "warn".into(),
                fix: None,
            },
            Check {
                name: "c".into(),
                status: CheckStatus::Fail,
                message: "fail".into(),
                fix: None,
            },
        ];
        let (has_fail, summary) = build_summary(&checks);
        assert!(has_fail);
        assert_eq!(summary, "1 passed, 1 warnings, 1 failed");
    }

    #[test]
    fn lint_10_check_security_mode_no_config() {
        let path = PathBuf::from("/nonexistent/forge.toml");
        let check = check_security_mode(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn lint_11_check_security_mode_no_remote_includes() {
        let dir = std::env::temp_dir().join("forge-lint-test-secmode-local");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
security_mode = "strict"

[servers.test]
command = "test"
transport = "stdio"
"#,
        )
        .unwrap();
        let check = check_security_mode(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
        assert!(check.message.contains("no remote includes"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_12_circuit_breakers_warns_unprotected_sse() {
        let dir = std::env::temp_dir().join("forge-lint-test-cb-warn");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.remote]
transport = "sse"
url = "http://example.com/sse"
"#,
        )
        .unwrap();
        let check = check_circuit_breakers(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(
            check.message.contains("remote"),
            "should mention server name: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_13_token_formats_detects_quoted_token() {
        let dir = std::env::temp_dir().join("forge-lint-test-token-quote");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.test]
transport = "sse"
url = "http://example.com/sse"
headers = { x-api-key = "\"my-key\"" }
"#,
        )
        .unwrap();
        let check = check_token_formats(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(check.message.contains("quotes"), "{}", check.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_14_config_with_includes_shows_count() {
        let dir = std::env::temp_dir().join("forge-lint-test-inc-count");
        std::fs::create_dir_all(&dir).unwrap();

        let included = dir.join("shared.toml");
        std::fs::write(
            &included,
            "[servers.shared]\ncommand = \"s\"\ntransport = \"stdio\"\n",
        )
        .unwrap();

        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            "[[include]]\npath = \"./shared.toml\"\n",
        )
        .unwrap();

        let check = check_config_valid(Some(&main_path));
        assert_eq!(check.status, CheckStatus::Pass);
        assert!(
            check.message.contains("1 include"),
            "should mention include count: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_15_run_config_checks_returns_all() {
        let dir = std::env::temp_dir().join("forge-lint-test-all-checks");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\n",
        )
        .unwrap();
        let checks = run_config_checks(Some(&path));
        // Should have at least 7 config checks
        assert!(
            checks.len() >= 7,
            "expected at least 7 checks, got {}",
            checks.len()
        );
        // Verify all check names are unique
        let names: Vec<&str> = checks.iter().map(|c| c.name.as_str()).collect();
        let unique: std::collections::HashSet<&str> = names.iter().copied().collect();
        assert_eq!(
            names.len(),
            unique.len(),
            "check names should be unique: {:?}",
            names
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lint_16_check_env_vars_scans_includes() {
        let dir = std::env::temp_dir().join("forge-lint-test-env-includes");
        std::fs::create_dir_all(&dir).unwrap();

        // Create an included file with an env var reference
        let included_path = dir.join("shared.toml");
        std::fs::write(
            &included_path,
            "[servers.inc]\ncommand = \"cmd\"\ntransport = \"stdio\"\nheaders = { Auth = \"${FORGE_LINT_INCLUDED_VAR}\" }\n",
        )
        .unwrap();

        // Create main config that includes the above
        let main_path = dir.join("forge.toml");
        std::fs::write(
            &main_path,
            "[[include]]\npath = \"./shared.toml\"\n",
        )
        .unwrap();

        let check = check_env_vars(Some(&main_path));
        assert_eq!(check.status, CheckStatus::Fail, "{}", check.message);
        assert!(
            check.message.contains("FORGE_LINT_INCLUDED_VAR"),
            "should detect env var in included file: {}",
            check.message
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
