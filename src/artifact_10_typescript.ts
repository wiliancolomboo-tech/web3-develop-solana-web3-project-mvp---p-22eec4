// File: src/relayer/health.ts
import http from "http";

const PORT = process.env.PORT || 8080;

export function initHealthServer(checkHealth: () => boolean) {
  const server = http.createServer((req, res) => {
    if (req.url === "/healthz") {
      if (checkHealth()) {
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "healthy", timestamp: Date.now() }));
      } else {
        res.writeHead(503, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "unhealthy", reason: "RPC connection dropped" }));
      }
    } else {
      res.writeHead(404);
      res.end();
    }
  });

  server.listen(PORT, () => {
    console.log(`[HealthCheck] Server listening on :${PORT}`);
  });
}