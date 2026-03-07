// This script runs all doc generation scripts after Docusaurus build.
const { execSync } = require('child_process');
const path = require('path');

function run(script) {
  execSync(`node ${script}`, { stdio: 'inherit', cwd: __dirname });
}

function main() {
  run('./generate-api-docs.js');
  run('./generate-cli-docs.js');
  run('./generate-config-docs.js');
}

main();
