# Forgemax Roadmap

## v0.4.0 — Platform Release (current)

- CLI subcommands: `doctor`, `manifest`, `run`, `init`
- Production features default-on: worker-pool, metrics, config-watch
- Documentation: SECURITY.md, CONTRIBUTING.md, examples/
- Production configuration template

## v0.5.0 — Observability

- Prometheus metrics endpoint (HTTP)
- OpenTelemetry tracing integration
- Structured audit log output (JSON)
- Health check endpoint
- Dashboard templates (Grafana)

## v0.6.0 — Ecosystem

- Plugin system for custom validators
- Server discovery (auto-detect MCP servers on PATH)
- Configuration profiles (dev/staging/production)
- Streamable HTTP transport support

## v1.0.0 — Stability

- Stable public API (SemVer guarantee)
- Long-term support commitment
- Comprehensive migration guides
- Performance benchmarks and optimization

## Non-Goals

The following are explicitly out of scope:

- **VS Code / IDE extension** — Forgemax is a protocol-level gateway, not an editor plugin
- **GUI / TUI** — CLI-first design; use `forgemax doctor --json` and pipe to your preferred UI
- **Telemetry / analytics** — No data leaves your machine unless you configure a downstream server
- **Built-in lodash / utility libraries in sandbox** — Keep the sandbox minimal; LLMs can write vanilla JS
- **Plugin system for sandbox extensions** — Security boundary must remain simple and auditable
