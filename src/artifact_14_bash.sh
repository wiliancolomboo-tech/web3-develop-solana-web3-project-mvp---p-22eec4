# Configure environment parameters
cp .env.example .env

# Spin up self-healing telemetry daemon
docker compose up -d --build
docker compose logs -f campus-vibe-relayer