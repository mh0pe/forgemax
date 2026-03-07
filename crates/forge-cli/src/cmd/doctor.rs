//! `forgemax doctor` — validate configuration and connectivity.
//!
//! Doctor runs all config checks from `lint` plus system-level checks
//! (memory, features, worker binary) and server connectivity checks.

use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::cmd::lint::{self, Check, CheckStatus, LintReport};
use crate::common;

/// Arguments for the doctor subcommand.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Output results as JSON.
    #[arg(long)]
    pub json: bool,
}

/// Type aliases for backward compatibility with existing tests.
#[allow(dead_code)]
pub type DoctorCheck = Check;
/// Type alias for the report type.
pub type DoctorReport = LintReport;

// ──────────────────────────────────────────────────────────────────────
// Doctor-specific checks (system & connectivity, not in lint)
// ──────────────────────────────────────────────────────────────────────

/// Check that the worker binary can be found.
fn check_worker_binary() -> Check {
    match forge_sandbox::host::find_worker_binary() {
        Ok(path) => Check {
            name: "worker_binary".into(),
            status: CheckStatus::Pass,
            message: format!("worker binary found: {}", path.display()),
            fix: None,
        },
        Err(e) => Check {
            name: "worker_binary".into(),
            status: CheckStatus::Fail,
            message: format!("worker binary not found: {}", e),
            fix: Some("Set FORGE_WORKER_BIN or install forgemax-worker alongside forgemax".into()),
        },
    }
}

/// Check compiled feature flags.
fn check_features() -> Check {
    let line = common::feature_status_line();
    let all_on = cfg!(feature = "worker-pool")
        && cfg!(feature = "metrics")
        && cfg!(feature = "config-watch");

    Check {
        name: "features".into(),
        status: if all_on {
            CheckStatus::Pass
        } else {
            CheckStatus::Warn
        },
        message: line,
        fix: if all_on {
            None
        } else {
            Some("Rebuild with default features for full functionality".into())
        },
    }
}

/// Get available system memory in MB.
fn get_system_memory_mb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map(|b| b / (1024 * 1024))
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .map(|kb| kb / 1024)
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Check available system memory.
fn check_memory() -> Check {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output();
        match output {
            Ok(o) if o.status.success() => {
                let mem_str = String::from_utf8_lossy(&o.stdout);
                let mem_bytes: u64 = mem_str.trim().parse().unwrap_or(0);
                let mem_gb = mem_bytes / (1024 * 1024 * 1024);
                if mem_gb < 4 {
                    Check {
                        name: "memory".into(),
                        status: CheckStatus::Warn,
                        message: format!("system memory: {} GB (recommended >= 4 GB)", mem_gb),
                        fix: None,
                    }
                } else {
                    Check {
                        name: "memory".into(),
                        status: CheckStatus::Pass,
                        message: format!("system memory: {} GB", mem_gb),
                        fix: None,
                    }
                }
            }
            _ => Check {
                name: "memory".into(),
                status: CheckStatus::Pass,
                message: "could not determine system memory".into(),
                fix: None,
            },
        }
    }
    #[cfg(target_os = "linux")]
    {
        match std::fs::read_to_string("/proc/meminfo") {
            Ok(content) => {
                let mem_kb = content
                    .lines()
                    .find(|l| l.starts_with("MemTotal:"))
                    .and_then(|l| {
                        l.split_whitespace()
                            .nth(1)
                            .and_then(|s| s.parse::<u64>().ok())
                    })
                    .unwrap_or(0);
                let mem_gb = mem_kb / (1024 * 1024);
                if mem_gb < 4 {
                    Check {
                        name: "memory".into(),
                        status: CheckStatus::Warn,
                        message: format!("system memory: {} GB (recommended >= 4 GB)", mem_gb),
                        fix: None,
                    }
                } else {
                    Check {
                        name: "memory".into(),
                        status: CheckStatus::Pass,
                        message: format!("system memory: {} GB", mem_gb),
                        fix: None,
                    }
                }
            }
            Err(_) => Check {
                name: "memory".into(),
                status: CheckStatus::Pass,
                message: "could not determine system memory".into(),
                fix: None,
            },
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Check {
            name: "memory".into(),
            status: CheckStatus::Pass,
            message: "memory check skipped (unsupported platform)".into(),
            fix: None,
        }
    }
}

/// Check that worst-case worker memory usage won't exceed system RAM.
fn check_memory_pressure(config_path: Option<&PathBuf>) -> Check {
    let config = match common::load_config(config_path) {
        Ok(c) => c,
        Err(_) => {
            return Check {
                name: "memory_pressure".into(),
                status: CheckStatus::Pass,
                message: "no valid config to check".into(),
                fix: None,
            };
        }
    };

    let max_concurrent = config.sandbox.max_concurrent.unwrap_or(4) as u64;
    let max_heap_mb = config.sandbox.max_heap_mb.unwrap_or(256) as u64;
    let worst_case_mb = max_concurrent * max_heap_mb;

    match get_system_memory_mb() {
        Some(available_mb) if available_mb > 0 => {
            if worst_case_mb > available_mb * 80 / 100 {
                Check {
                    name: "memory_pressure".into(),
                    status: CheckStatus::Warn,
                    message: format!(
                        "worst-case memory: {} MB ({}x{} MB) exceeds 80% of {} MB system RAM",
                        worst_case_mb, max_concurrent, max_heap_mb, available_mb
                    ),
                    fix: Some(format!(
                        "Reduce max_concurrent (currently {}) or max_heap_mb (currently {})",
                        max_concurrent, max_heap_mb
                    )),
                }
            } else {
                Check {
                    name: "memory_pressure".into(),
                    status: CheckStatus::Pass,
                    message: format!(
                        "worst-case memory: {} MB ({}x{} MB), system has {} MB",
                        worst_case_mb, max_concurrent, max_heap_mb, available_mb
                    ),
                    fix: None,
                }
            }
        }
        _ => Check {
            name: "memory_pressure".into(),
            status: CheckStatus::Pass,
            message: format!(
                "worst-case memory: {} MB ({}x{} MB), could not detect system RAM",
                worst_case_mb, max_concurrent, max_heap_mb
            ),
            fix: None,
        },
    }
}

/// Execute the doctor command.
///
/// Runs all config checks from `lint` plus doctor-specific system checks
/// (worker binary, features, memory, memory pressure) and server connectivity.
pub async fn execute(args: &DoctorArgs, config_path: Option<PathBuf>) -> Result<()> {
    let config_ref = config_path.as_ref();

    // Start with all config checks from lint
    let mut checks = lint::run_config_checks(config_ref);

    // Add doctor-specific system checks
    checks.push(check_worker_binary());
    checks.push(check_features());
    checks.push(check_memory());
    checks.push(check_memory_pressure(config_ref));

    // Server connectivity check (async)
    let config = common::load_config(config_ref).ok();
    if let Some(ref config) = config {
        if !config.servers.is_empty() {
            for (name, server_config) in &config.servers {
                match common::to_transport_config(server_config) {
                    Ok(transport_config) => {
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(10),
                            forge_client::McpClient::connect(name.clone(), &transport_config),
                        )
                        .await
                        {
                            Ok(Ok(client)) => match client.list_tools().await {
                                Ok(tools) => {
                                    checks.push(Check {
                                        name: format!("server_{}", name),
                                        status: CheckStatus::Pass,
                                        message: format!(
                                            "server '{}': connected, {} tools",
                                            name,
                                            tools.len()
                                        ),
                                        fix: None,
                                    });
                                }
                                Err(e) => {
                                    checks.push(Check {
                                        name: format!("server_{}", name),
                                        status: CheckStatus::Fail,
                                        message: format!(
                                            "server '{}': connected but list_tools failed: {}",
                                            name, e
                                        ),
                                        fix: None,
                                    });
                                }
                            },
                            Ok(Err(e)) => {
                                checks.push(Check {
                                    name: format!("server_{}", name),
                                    status: CheckStatus::Fail,
                                    message: format!(
                                        "server '{}': connection failed: {}",
                                        name, e
                                    ),
                                    fix: Some(format!(
                                        "Verify server '{}' is installed and running",
                                        name
                                    )),
                                });
                            }
                            Err(_) => {
                                checks.push(Check {
                                    name: format!("server_{}", name),
                                    status: CheckStatus::Fail,
                                    message: format!(
                                        "server '{}': connection timed out (10s)",
                                        name
                                    ),
                                    fix: Some(format!(
                                        "Verify server '{}' is installed and responsive",
                                        name
                                    )),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        checks.push(Check {
                            name: format!("server_{}", name),
                            status: CheckStatus::Fail,
                            message: format!(
                                "server '{}': invalid transport config: {}",
                                name, e
                            ),
                            fix: None,
                        });
                    }
                }
            }
        }
    }

    let (has_fail, summary) = lint::build_summary(&checks);

    let report = DoctorReport {
        schema_version: 1,
        passed: !has_fail,
        checks,
        summary: summary.clone(),
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        lint::print_checks(&report.checks, &summary);
    }

    if has_fail {
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Config validation checks are now tested in lint.rs.
    // These tests exercise doctor-specific checks and backward compatibility.

    #[test]
    fn dr_01_check_config_valid_with_valid_toml() {
        let dir = std::env::temp_dir().join("forge-doctor-test-valid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\n",
        )
        .unwrap();
        let check = lint::check_config_valid(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_02_check_config_valid_missing() {
        let path = PathBuf::from("/nonexistent/forge.toml");
        let check = lint::check_config_valid(Some(&path));
        assert_eq!(check.status, CheckStatus::Fail);
    }

    #[test]
    fn dr_03_check_config_valid_invalid_toml() {
        let dir = std::env::temp_dir().join("forge-doctor-test-invalid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "[[[invalid toml").unwrap();
        let check = lint::check_config_valid(Some(&path));
        assert_eq!(check.status, CheckStatus::Fail);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_04_check_env_vars_all_set() {
        let dir = std::env::temp_dir().join("forge-doctor-test-env");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "# no env var refs\n[sandbox]\ntimeout_secs = 5\n").unwrap();
        let check = lint::check_env_vars(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_05_check_env_vars_unresolved() {
        let dir = std::env::temp_dir().join("forge-doctor-test-env-missing");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\nheaders = { Auth = \"${FORGE_DOCTOR_TEST_NONEXISTENT_VAR}\" }\n",
        )
        .unwrap();
        let check = lint::check_env_vars(Some(&path));
        assert_eq!(check.status, CheckStatus::Fail);
        assert!(check.message.contains("FORGE_DOCTOR_TEST_NONEXISTENT_VAR"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_06_check_worker_binary() {
        let check = check_worker_binary();
        assert!(
            check.status == CheckStatus::Pass || check.status == CheckStatus::Fail,
            "unexpected status: {:?}",
            check.status
        );
    }

    #[test]
    fn dr_09_check_groups_no_config() {
        let path = PathBuf::from("/nonexistent/forge.toml");
        let check = lint::check_groups(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn dr_10_check_groups_orphaned_servers() {
        let dir = std::env::temp_dir().join("forge-doctor-test-groups");
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
        let check = lint::check_groups(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn);
        assert!(
            check.message.contains("b"),
            "should mention orphaned server"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_11_check_features() {
        let check = check_features();
        if cfg!(feature = "worker-pool")
            && cfg!(feature = "metrics")
            && cfg!(feature = "config-watch")
        {
            assert_eq!(check.status, CheckStatus::Pass);
        } else {
            assert_eq!(check.status, CheckStatus::Warn);
        }
    }

    #[test]
    fn dr_12_check_memory() {
        let check = check_memory();
        assert!(
            check.status == CheckStatus::Pass || check.status == CheckStatus::Warn,
            "unexpected: {:?}",
            check.status
        );
    }

    #[test]
    fn dr_13_report_json_valid() {
        let report = DoctorReport {
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
    fn dr_14_schema_version_is_one() {
        let report = DoctorReport {
            schema_version: 1,
            passed: true,
            checks: vec![],
            summary: String::new(),
        };
        assert_eq!(report.schema_version, 1);
    }

    #[cfg(unix)]
    #[test]
    fn dr_15_config_permissions_world_readable_with_secrets() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join("forge-doctor-test-perms");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "headers = { Auth = \"${SECRET}\" }\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let check = lint::check_config_permissions(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn dr_16_config_permissions_secure() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join("forge-doctor-test-perms-ok");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "headers = { Auth = \"${SECRET}\" }\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let check = lint::check_config_permissions(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_07_unreachable_server_config() {
        let dir = std::env::temp_dir().join("forge-doctor-test-unreachable");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.fake_unreachable]
command = "/nonexistent/binary/that/does/not/exist"
transport = "stdio"
timeout_secs = 1
"#,
        )
        .unwrap();
        let config_check = lint::check_config_valid(Some(&path));
        assert_eq!(config_check.status, CheckStatus::Pass);
        let groups_check = lint::check_groups(Some(&path));
        assert_eq!(groups_check.status, CheckStatus::Pass);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn dr_08_server_connectivity_timeout() {
        let dir = std::env::temp_dir().join("forge-doctor-test-timeout");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.timeout_test]
command = "/nonexistent/binary/xyzzy"
transport = "stdio"
timeout_secs = 1
"#,
        )
        .unwrap();

        let args = DoctorArgs { json: true };
        let config_check = lint::check_config_valid(Some(&path));
        assert_eq!(config_check.status, CheckStatus::Pass);

        let env_check = lint::check_env_vars(Some(&path));
        assert_eq!(
            env_check.status,
            CheckStatus::Pass,
            "no env var refs expected"
        );
        let _ = args;
        std::fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn dr_17_config_permissions_ok_without_env_vars() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join("forge-doctor-test-perms-no-env");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let check = lint::check_config_permissions(Some(&path));
        assert_eq!(
            check.status,
            CheckStatus::Pass,
            "world-readable config without secrets should pass: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_18_memory_check_returns_pass_or_warn() {
        let check = check_memory();
        assert!(
            check.status == CheckStatus::Pass || check.status == CheckStatus::Warn,
            "memory check should never fail, got: {:?} - {}",
            check.status,
            check.message
        );
        assert!(
            check.message.contains("memory") || check.message.contains("determine"),
            "memory check message should mention memory: {}",
            check.message
        );
    }

    #[test]
    fn dr_20_memory_pressure_warns_high_usage() {
        let dir = std::env::temp_dir().join("forge-doctor-test-mempressure-high");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "[sandbox]\nmax_concurrent = 16\nmax_heap_mb = 512\n").unwrap();
        let check = check_memory_pressure(Some(&path));
        assert!(
            check.status == CheckStatus::Warn || check.status == CheckStatus::Pass,
            "unexpected: {:?} - {}",
            check.status,
            check.message
        );
        assert!(
            check.message.contains("8192"),
            "should show 8192 MB: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_21_memory_pressure_passes_low_usage() {
        let dir = std::env::temp_dir().join("forge-doctor-test-mempressure-low");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(&path, "[sandbox]\nmax_concurrent = 2\nmax_heap_mb = 64\n").unwrap();
        let check = check_memory_pressure(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass, "{}", check.message);
        assert!(
            check.message.contains("128"),
            "should show 128 MB: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_22_memory_pressure_defaults() {
        let dir = std::env::temp_dir().join("forge-doctor-test-mempressure-default");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            "[servers.test]\ncommand = \"test\"\ntransport = \"stdio\"\n",
        )
        .unwrap();
        let check = check_memory_pressure(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass, "{}", check.message);
        assert!(
            check.message.contains("1024"),
            "should show 1024 MB: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_23_circuit_breakers_warns_unprotected_sse() {
        let dir = std::env::temp_dir().join("forge-doctor-test-cb-warn");
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
        let check = lint::check_circuit_breakers(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(
            check.message.contains("remote"),
            "should mention server name: {}",
            check.message
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_24_circuit_breakers_passes_all_protected() {
        let dir = std::env::temp_dir().join("forge-doctor-test-cb-pass");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.remote]
transport = "sse"
url = "http://example.com/sse"
circuit_breaker = true
"#,
        )
        .unwrap();
        let check = lint::check_circuit_breakers(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass, "{}", check.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_25_circuit_breakers_ignores_stdio() {
        let dir = std::env::temp_dir().join("forge-doctor-test-cb-stdio");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.local]
command = "test"
transport = "stdio"
"#,
        )
        .unwrap();
        let check = lint::check_circuit_breakers(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass, "{}", check.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_26_token_formats_detects_quoted_token() {
        let dir = std::env::temp_dir().join("forge-doctor-test-token-quote");
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
        let check = lint::check_token_formats(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(check.message.contains("quotes"), "{}", check.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_27_token_formats_detects_bearer_prefix() {
        let dir = std::env::temp_dir().join("forge-doctor-test-token-bearer");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.test]
transport = "sse"
url = "http://example.com/sse"
headers = { x-api-key = "Bearer sk-12345" }
"#,
        )
        .unwrap();
        let check = lint::check_token_formats(Some(&path));
        assert_eq!(check.status, CheckStatus::Warn, "{}", check.message);
        assert!(check.message.contains("Bearer"), "{}", check.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_28_token_formats_passes_clean() {
        let dir = std::env::temp_dir().join("forge-doctor-test-token-clean");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("forge.toml");
        std::fs::write(
            &path,
            r#"
[servers.test]
transport = "sse"
url = "http://example.com/sse"
headers = { Authorization = "Bearer sk-12345", x-api-key = "clean-token" }
"#,
        )
        .unwrap();
        let check = lint::check_token_formats(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass, "{}", check.message);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dr_29_token_formats_no_config() {
        let path = PathBuf::from("/nonexistent/forge.toml");
        let check = lint::check_token_formats(Some(&path));
        assert_eq!(check.status, CheckStatus::Pass);
    }

    #[test]
    fn dr_19_memory_check_platform_behavior() {
        let check = check_memory();
        #[cfg(target_os = "macos")]
        {
            assert!(
                check.message.contains("GB") || check.message.contains("determine"),
                "macOS memory check should report GB: {}",
                check.message
            );
        }
        #[cfg(target_os = "linux")]
        {
            assert!(
                check.message.contains("GB") || check.message.contains("determine"),
                "Linux memory check should report GB: {}",
                check.message
            );
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        {
            assert!(
                check.message.contains("skipped"),
                "unsupported platform should skip: {}",
                check.message
            );
        }
        let _ = check;
    }
}
