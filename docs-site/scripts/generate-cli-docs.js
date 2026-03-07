// This script generates CLI documentation by running --help on the forgemax binary.
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function main() {
  try {
    const help = execSync('cargo run --bin forgemax -- --help', {
      cwd: path.resolve(__dirname, '../../'),
      encoding: 'utf-8',
    });
    fs.writeFileSync(path.resolve(__dirname, '../docs/cli-help.txt'), help);
    console.log('CLI help written to docs/cli-help.txt');
  } catch (e) {
    console.error('Failed to generate CLI help:', e);
  }
}

main();
