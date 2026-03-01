// @prompt  Use the fluent server proxy API to call tools across servers
// @features forge.server() proxy API
// @output  Combined results from multiple servers
// @servers narsil, github

async () => {
  // Fluent API: forge.server("name").category.tool(args)
  const codeSearch = await forge.server("narsil").symbols.find({
    pattern: "handleRequest",
    limit: 3,
  });

  // Chain to another server
  const issues = await forge.server("github").issues.list({
    repo: "postrv/forgemax",
    state: "open",
    limit: 5,
  });

  return {
    symbols_found: codeSearch.symbols ? codeSearch.symbols.length : 0,
    open_issues: issues.issues ? issues.issues.length : 0,
  };
};
