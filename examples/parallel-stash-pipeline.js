// @prompt  Hero example: parallel fan-out, stash results, then consume
// @features forge.parallel(), forge.stash, forge.callTool
// @output  Aggregated analysis results
// @servers narsil

async () => {
  // Phase 1: Fan out discovery across multiple patterns in parallel
  const discoveries = await forge.parallel([
    { fn: () => forge.callTool("narsil", "find_symbols", { pattern: "export function", limit: 10 }) },
    { fn: () => forge.callTool("narsil", "find_symbols", { pattern: "export class", limit: 10 }) },
    { fn: () => forge.callTool("narsil", "find_symbols", { pattern: "export interface", limit: 10 }) },
  ], { concurrency: 3 });

  // Phase 2: Stash the results for later use
  await forge.stash.put("discovered_functions", discoveries[0], { ttl: 3600 });
  await forge.stash.put("discovered_classes", discoveries[1], { ttl: 3600 });
  await forge.stash.put("discovered_interfaces", discoveries[2], { ttl: 3600 });

  // Phase 3: Consume from stash and aggregate
  const functions = await forge.stash.get("discovered_functions");
  const classes = await forge.stash.get("discovered_classes");
  const interfaces = await forge.stash.get("discovered_interfaces");

  const functionCount = functions && functions.symbols ? functions.symbols.length : 0;
  const classCount = classes && classes.symbols ? classes.symbols.length : 0;
  const interfaceCount = interfaces && interfaces.symbols ? interfaces.symbols.length : 0;

  return {
    summary: {
      total: functionCount + classCount + interfaceCount,
      functions: functionCount,
      classes: classCount,
      interfaces: interfaceCount,
    },
    stash_keys: await forge.stash.keys(),
  };
};
