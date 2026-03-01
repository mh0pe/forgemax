// @prompt  Demonstrate the session stash for caching across executions
// @features forge.stash.put, forge.stash.get, forge.stash.keys
// @output  Stash state after operations
// @servers (any)

async () => {
  // Store a value with a 1-hour TTL
  await forge.stash.put("schema_cache", {
    tables: ["users", "orders", "products"],
    discovered_at: new Date().toISOString(),
  }, { ttl: 3600 });

  // Store another value
  await forge.stash.put("last_query", {
    sql: "SELECT * FROM users LIMIT 10",
    rows: 10,
  });

  // Retrieve a value
  const cached = await forge.stash.get("schema_cache");

  // List all keys
  const keys = await forge.stash.keys();

  return {
    cached_schema: cached,
    all_keys: keys,
  };

  // In a subsequent execute() call, the stash persists:
  // const schema = await forge.stash.get("schema_cache");
  // if (schema) { /* use cached schema */ }
};
