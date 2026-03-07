// This script copies example TOML configs into the docs for reference.
const fs = require('fs');
const path = require('path');

function copyConfig(src, dest) {
  fs.copyFileSync(src, dest);
  console.log(`Copied ${src} to ${dest}`);
}

function main() {
  const root = path.resolve(__dirname, '../../');
  copyConfig(path.join(root, 'forge.toml.example'), path.join(__dirname, '../docs/forge.toml.example'));
  copyConfig(path.join(root, 'forge.toml.example.production'), path.join(__dirname, '../docs/forge.toml.example.production'));
}

main();
