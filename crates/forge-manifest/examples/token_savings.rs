//! Measures token savings by comparing raw MCP schema size vs. Forgemax schema size.
//!
//! Run: `cargo run -p forge-manifest --example token_savings`

use forge_manifest::{Category, ManifestBuilder, ParamDef, ServerBuilder, ToolEntry, FORGE_DTS};

/// Generate a mock manifest with `num_servers` servers, each with `tools_per_server` tools.
fn generate_manifest(num_servers: usize, tools_per_server: usize) -> forge_manifest::Manifest {
    let mut builder = ManifestBuilder::new();

    for s in 0..num_servers {
        let mut categories = Vec::new();
        // Split tools across a few categories
        let cats_per_server = (tools_per_server / 5).max(1);
        let tools_per_cat = tools_per_server / cats_per_server;

        for c in 0..cats_per_server {
            let tools: Vec<ToolEntry> = (0..tools_per_cat)
                .map(|t| ToolEntry {
                    name: format!("tool_{t}"),
                    description: format!(
                        "Performs operation {t} on the data. Accepts various parameters \
                         and returns structured results."
                    ),
                    params: vec![
                        ParamDef {
                            name: "input".into(),
                            param_type: "string".into(),
                            required: true,
                            description: Some("The input data to process".into()),
                        },
                        ParamDef {
                            name: "options".into(),
                            param_type: "object".into(),
                            required: false,
                            description: Some("Optional configuration for the operation".into()),
                        },
                    ],
                    returns: Some("Processed result as JSON".into()),
                    input_schema: Some(serde_json::json!({
                        "type": "object",
                        "properties": {
                            "input": { "type": "string", "description": "The input data" },
                            "options": {
                                "type": "object",
                                "properties": {
                                    "format": { "type": "string", "enum": ["json", "text", "csv"] },
                                    "verbose": { "type": "boolean", "default": false }
                                }
                            }
                        },
                        "required": ["input"]
                    })),
                })
                .collect();

            categories.push(Category {
                name: format!("category_{c}"),
                description: format!("Category {c} tools for server {s}"),
                tools,
            });
        }

        let mut server_builder = ServerBuilder::new(
            format!("server-{s}"),
            format!("Server {s} providing various tools for data processing and analysis"),
        );
        for cat in categories {
            server_builder = server_builder.add_category(cat);
        }
        builder = builder.add_server(server_builder.build());
    }

    builder.build()
}

/// Estimate token count. Conservative: ~4 chars per token for JSON schema text.
fn estimate_tokens(text: &str) -> usize {
    // This is a rough approximation. Real tokenizers vary, but for JSON schema
    // content, 4 chars/token is a reasonable conservative estimate.
    text.len() / 4
}

fn main() {
    let scenarios = vec![
        ("10 tools (2 servers)", 2, 5),
        ("25 tools (5 servers)", 5, 5),
        ("50 tools (5 servers)", 5, 10),
        ("76 tools (8 servers)", 8, 10),
        ("100 tools (10 servers)", 10, 10),
        ("150 tools (10 servers)", 10, 15),
        ("200 tools (10 servers)", 10, 20),
    ];

    // The Forgemax schema is always 2 tools (search + execute) plus the DTS
    // and instructions. Measure the fixed overhead once.
    let forge_schema = format!(
        r#"Tools: search(code: string), execute(code: string)
Instructions: Forgemax Code Mode Gateway. Use search() to discover, execute() to call.

TypeScript Definitions:
```typescript
{}
```"#,
        FORGE_DTS
    );
    let forge_tokens = estimate_tokens(&forge_schema);

    println!("# Forgemax Token Savings Report\n");
    println!(
        "Forgemax schema size: {} chars (~{} tokens)\n",
        forge_schema.len(),
        forge_tokens
    );
    println!("| Scenario | Total Tools | Raw MCP (tokens) | Forgemax (tokens) | Savings |");
    println!("|----------|------------|------------------|-------------------|---------|");

    for (name, servers, tools_per) in &scenarios {
        let manifest = generate_manifest(*servers, *tools_per);
        let total_tools = manifest.total_tools();

        // Raw MCP schema = full JSON Schema for every tool (what an LLM sees without Forgemax)
        let raw_json = serde_json::to_string_pretty(&manifest.to_json().unwrap()).unwrap();
        let raw_tokens = estimate_tokens(&raw_json);

        let savings_pct = if raw_tokens > 0 {
            ((1.0 - forge_tokens as f64 / raw_tokens as f64) * 100.0) as u32
        } else {
            0
        };

        println!("| {name} | {total_tools} | ~{raw_tokens} | ~{forge_tokens} | {savings_pct}% |");
    }

    println!("\n*Estimates based on ~4 chars/token for JSON schema content.*");
    println!("*Forgemax schema is constant regardless of tool count.*");
}
