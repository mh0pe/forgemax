// @prompt  Fan out multiple independent tool calls in parallel
// @features forge.parallel()
// @output  Combined results from parallel execution
// @servers narsil

async () => {
  // forge.parallel() takes an array of {fn: () => Promise} objects
  // and executes them concurrently up to the configured limit
  const results = await forge.parallel([
    { fn: () => forge.callTool("narsil", "find_symbols", { pattern: "class", limit: 5 }) },
    { fn: () => forge.callTool("narsil", "find_symbols", { pattern: "interface", limit: 5 }) },
    { fn: () => forge.callTool("narsil", "find_symbols", { pattern: "enum", limit: 5 }) },
  ], { concurrency: 3 });

  return {
    classes: results[0],
    interfaces: results[1],
    enums: results[2],
  };
};
