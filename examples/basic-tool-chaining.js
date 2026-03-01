// @prompt  Chain two tool calls: find symbols then get details
// @features forge.callTool, manifest
// @output  JSON array of symbol details
// @servers narsil (or any server with find_symbols/get_symbol tools)

async () => {
  // Step 1: Find all exported functions
  const symbols = await forge.callTool("narsil", "find_symbols", {
    pattern: "export function",
    limit: 5,
  });

  // Step 2: Get details for each symbol found
  const details = [];
  for (const sym of symbols.symbols || []) {
    const detail = await forge.callTool("narsil", "get_symbol", {
      name: sym.name,
      file: sym.file,
    });
    details.push(detail);
  }

  return details;
};
