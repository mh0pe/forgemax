// This script generates API documentation from Rust source code using rustdoc JSON output.
// It can be extended to parse CLI help, TOML config, and more.
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function runRustdocJson(cratePath, outFile) {
  try {
    execSync(`cargo +nightly rustdoc --lib -- -Z unstable-options --output-format json`, {
      cwd: cratePath,
      stdio: 'inherit',
    });
    // Find the generated JSON file
    const targetDir = path.join(cratePath, 'target', 'doc');
    const files = fs.readdirSync(targetDir);
    const jsonFile = files.find(f => f.endsWith('.json'));
    if (jsonFile) {
      fs.copyFileSync(path.join(targetDir, jsonFile), outFile);
      console.log(`API JSON written to ${outFile}`);
    }
  } catch (e) {
    console.error('Failed to generate rustdoc JSON:', e);
  }
}

function main() {
  // Example: generate docs for forge-cli
  const crate = path.resolve(__dirname, '../../crates/forge-cli');
  const out = path.resolve(__dirname, '../docs/api-forge-cli.json');
  runRustdocJson(crate, out);
  // TODO: parse CLI help, TOML config, etc.
}

main();
