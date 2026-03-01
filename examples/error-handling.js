// @prompt  Demonstrate structured error handling with fuzzy matching
// @features Structured errors, try/catch
// @output  Error details with suggestions
// @servers narsil

async () => {
  const errors = [];

  // Deliberate typo: "find_symbls" instead of "find_symbols"
  // The gateway returns a structured error with a fuzzy-match suggestion
  try {
    await forge.callTool("narsil", "find_symbls", { pattern: "main" });
  } catch (e) {
    errors.push({
      type: "tool_not_found",
      message: e.message,
      // Error message will include: "Did you mean 'find_symbols'?"
    });
  }

  // Deliberate typo: "narsill" instead of "narsil"
  try {
    await forge.callTool("narsill", "find_symbols", { pattern: "main" });
  } catch (e) {
    errors.push({
      type: "server_not_found",
      message: e.message,
      // Error message will include: "Did you mean 'narsil'?"
    });
  }

  return { errors };
};
