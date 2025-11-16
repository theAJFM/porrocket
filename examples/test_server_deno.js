// Ultra-simple Deno TCP server using low-level API
// Usage: deno run --allow-net test_server_deno.js [port]

const port = Deno.args[0] ? parseInt(Deno.args[0]) : 8000;

console.log(`Listening on port ${port}`);

const listener = Deno.listen({ port, hostname: "0.0.0.0" });

for await (const conn of listener) {
  // Handle connection asynchronously
  (async () => {
    try {
      const response = new TextEncoder().encode("HTTP/1.0 200 OK\r\n\r\nHello from Deno!\n");
      await conn.write(response);
      conn.close();
    } catch (err) {
      console.error("Connection error:", err);
    }
  })();
}
