// @prompt  Read a resource from a downstream server and filter in-sandbox
// @features forge.readResource()
// @output  Filtered resource content
// @servers postgres (or any server exposing resources)

async () => {
  // Read a resource by URI from a downstream server
  const logs = await forge.readResource("postgres", "file:///logs/app.log");

  // Filter and process the content in the sandbox
  const content = typeof logs === "string" ? logs : JSON.stringify(logs);
  const errorLines = content
    .split("\n")
    .filter((line) => line.includes("ERROR") || line.includes("FATAL"));

  return {
    total_lines: content.split("\n").length,
    error_count: errorLines.length,
    recent_errors: errorLines.slice(-5),
  };
};
