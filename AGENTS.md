# AGENTS.md

Standards and workflows for AI agents operating in this repository.

## Repo-Wide Standards

### Language & Tooling

- **Language**: Rust (edition 2021, MSRV 1.91.1)
- **Build**: `cargo build --workspace`
- **Test**: `cargo test --workspace`
- **Lint**: `cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings`
- **Security audit**: `cargo audit`
- **Dependency policy**: `cargo deny check --all-features`

### Coding Conventions

- **Error handling**: Use typed errors (`thiserror` enums), never `unwrap()` or `expect()` in production code. Clippy enforces `unwrap_used`, `expect_used`, and `panic` as warnings, and CI promotes warnings to errors via `RUSTFLAGS=-D warnings`.
- **Async runtime**: Tokio. All async code uses `tokio::spawn`, `tokio::sync` primitives.
- **Serialization**: `serde` with `derive` for all public types that cross boundaries.
- **Logging**: `tracing` crate with structured spans. No `println!` in library code.
- **Dependencies**: Workspace-level dependency declarations in root `Cargo.toml`. Crates use `workspace = true` inheritance.
- **Feature flags**: Default features should be on for common functionality. Optional features for heavier dependencies (e.g., `metrics`, `config-watch`, `worker-pool`).

### Testing Conventions

- **Test ID prefixes**: Tests use domain-specific prefixes for traceability:
  - `ff_` — feature flag tests
  - `cv_` — code validation / AST tests
  - `dr_` — doctor command tests
  - `cli_` — CLI integration tests
- **Security tests**: Bypass attempts and sandbox escape tests live alongside the code they protect.
- **Mock server**: `forge-test-server` crate provides a mock MCP server for integration tests. Use it instead of spinning up real downstream servers.
- **Test count**: Never remove existing tests without explicit justification. Run `cargo test --workspace` to verify the current count.

### Security Conventions

- **Credentials**: Never hardcode API keys or tokens. Use `${ENV_VAR}` expansion in config files. CI scans for leaked credential patterns (20+ char API key strings).
- **Sandbox**: All LLM-generated code runs in a V8 isolate with no ambient capabilities. Changes to the sandbox security boundary require corresponding bypass tests.
- **Error redaction**: URLs, IPs, paths, credentials, and stack traces must be stripped before reaching the LLM. See `forge-error` for the redaction pipeline.
- **Input validation**: All external input (tool names, resource URIs, stash keys) must be validated before processing.

### Pull Request Process

1. Write or update tests first (TDD preferred).
2. Run `cargo fmt --all` and `cargo clippy --all-targets`.
3. Run `cargo test --workspace` and ensure all tests pass.
4. Update CHANGELOG.md if the change is user-visible.
5. Update relevant documentation if behavior changes.

### Documentation Standards

This repository maintains six core documentation files. All must stay accurate and consistent with each other and with the codebase.

| File | Purpose | Must reference |
|------|---------|----------------|
| `README.md` | User-facing overview, install, quick start, architecture | SECURITY.md, examples, CLI commands |
| `ARCHITECTURE.md` | System design, security layers, isolation model | Crate structure, config options |
| `CONTRIBUTING.md` | Dev setup, testing, PR process, release checklist | Test conventions, crate roles |
| `SECURITY.md` | Threat model, defense-in-depth, hardening checklist | Sandbox guarantees, known limitations |
| `ROADMAP.md` | Version milestones and non-goals | Current version features |
| `UPGRADE.md` | Migration guides per version | Breaking changes, new features |

CI enforces that these files exist and that README.md links to SECURITY.md and references `forgemax doctor`.

---

## Devil's Advocate Documentation Review

A systematic workflow for critically reviewing documentation to ensure thoroughness, correctness, and internal consistency. The goal is to challenge every claim, find gaps, and catch contradictions — not to rubber-stamp.

**Automated workflow**: `.github/workflows/doc-review.yml` runs the automatable portions of this review on every PR that touches documentation, and can be triggered manually via `workflow_dispatch` for a full or scoped review. The workflow produces GitHub annotations inline on files and a summary report.

### Philosophy

Approach documentation as a skeptical reviewer who assumes nothing is correct until verified against the source of truth (the code). Good documentation is accurate documentation. Every factual claim should be traceable to code, config, or CI.

### Scope

Review all six core documentation files plus supporting files:

1. `README.md`
2. `ARCHITECTURE.md`
3. `CONTRIBUTING.md`
4. `SECURITY.md`
5. `ROADMAP.md`
6. `UPGRADE.md`
7. `examples/*.js` (header annotations and code accuracy)
8. `forge.toml.example` / `forge.toml.example.production`
9. `AGENTS.md` (this file — review it too)

### Step 1: Factual Accuracy Audit

For each documentation file, verify every factual claim against the codebase:

- **Version numbers**: Do documented versions (Rust MSRV, dependency versions, workspace version) match `Cargo.toml`?
- **Crate names and roles**: Does the architecture description match the actual `[workspace.members]` in `Cargo.toml`?
- **Feature lists**: Are documented features actually implemented? Search for the relevant code.
- **CLI commands**: Do documented commands and flags match the `clap` definitions in `forge-cli`?
- **Config options**: Do documented TOML keys match the config structs in `forge-config`?
- **Test count claims**: Does the claimed test count in README.md and CONTRIBUTING.md match `cargo test --workspace 2>&1 | tail -5`?
- **Example file count**: Does the claimed example count in README.md match `ls examples/*.js | wc -l`?
- **Security layers**: Does the defense-in-depth count in SECURITY.md match the actual table rows?
- **Error variants**: Do documented `DispatchError` variants match the enum in `forge-error`?

### Step 2: Internal Consistency Check

Cross-reference documentation files against each other:

- Does the architecture described in README.md match ARCHITECTURE.md?
- Does the security model in README.md match SECURITY.md?
- Does the test convention section in CONTRIBUTING.md match actual test files?
- Does UPGRADE.md reference features that are actually documented in README.md?
- Does ROADMAP.md's "current version" section match what's in CHANGELOG.md?
- Are crate descriptions consistent across README.md, CONTRIBUTING.md, and ARCHITECTURE.md?

### Step 3: Completeness Assessment

Identify gaps — things that exist in code but are missing from documentation:

- **Undocumented public APIs**: Are there public types, traits, or functions not mentioned in any doc?
- **Undocumented config options**: Are there config fields in `forge-config` structs not documented in README.md or the example configs?
- **Undocumented CLI flags**: Are there clap-defined flags not listed in the CLI commands table?
- **Undocumented error codes**: Are there `DispatchError` variants or error codes not explained?
- **Missing migration notes**: Are there breaking changes between versions not covered in UPGRADE.md?
- **Missing security considerations**: Are there security-relevant code paths not addressed in SECURITY.md?

### Step 4: Contradiction Detection

Actively look for contradictions:

- Does one file say a feature is optional while another implies it's always on?
- Does the install section reference package names that don't match published package names?
- Do performance claims have supporting evidence or benchmarks?
- Do "non-goals" in ROADMAP.md conflict with implemented features?
- Are there deprecated features still documented as current?

### Step 5: Staleness Detection

Identify documentation that may be outdated:

- **Dependency versions**: Compare documented versions against `Cargo.toml` and `Cargo.lock`.
- **Feature flags**: Have default features changed since documentation was written?
- **Removed code**: Is there documentation for code that no longer exists?
- **Changed behavior**: Has the implementation diverged from what's documented?

### Step 6: Example Validation

Verify that code examples are correct and runnable:

- Do JavaScript examples in README.md use the correct API signatures?
- Do `examples/*.js` files have required `@prompt` and `@features` headers?
- Do example configs in README.md parse as valid TOML?
- Do install commands reference the correct package names and URLs?

### Output Format

After completing the review, produce a report with the following sections:

```markdown
## Documentation Review Report

### Critical Issues
Items that are factually incorrect or could mislead users.
- [ ] Issue description → file:line → suggested fix

### Inconsistencies
Contradictions between documentation files.
- [ ] Description → file1 vs file2 → resolution

### Gaps
Missing documentation for implemented features.
- [ ] What's missing → where it should go → why it matters

### Staleness
Outdated information that needs updating.
- [ ] What's stale → current value → where to fix

### Verified Correct
Claims that were verified against the codebase (for confidence tracking).
- [x] Claim → verification method
```

### When to Run This Review

The automated workflow (`.github/workflows/doc-review.yml`) runs automatically on PRs that touch documentation files. For manual/on-demand runs:

- **Full review**: Trigger `workflow_dispatch` with scope `full` before any release
- **Accuracy only**: Trigger with scope `accuracy` after changing dependency versions or adding crates
- **Consistency only**: Trigger with scope `consistency` after refactors that change public APIs
- **Staleness only**: Trigger with scope `staleness` as a quarterly hygiene check

The workflow automates Steps 1–4 and 6. Steps 3 (completeness) and 4 (contradiction detection) involving semantic analysis should be performed manually by an agent or reviewer using the checklists above.
