import http from "http";

const PORT = process.env.PORT || 8080;

export function startTelemetryEndpoint() {
  const server = http.createServer((req, res) => {
    if (req.url === "/healthz") {
      res.writeHead(200, { "Content-Type": "application/json" });
      res.end(JSON.stringify({ status: "healthy", uptime: process.uptime(), timestamp: Date.now() }));
    } else {
      res.writeHead(404);
      res.end();
    }
  });

  server.listen(PORT, () => {
    console.log(`[Telemetry] Healthcheck server live on :${PORT}/healthz`);
  });
}