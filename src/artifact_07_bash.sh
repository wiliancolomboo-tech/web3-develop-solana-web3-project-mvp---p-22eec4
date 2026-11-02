# 1. Build and verify the Solana program locally
anchor build

# 2. Deploy program to Solana Devnet
solana program deploy ./target/deploy/univibe.so \
  --program-id ./target/deploy/univibe-keypair.json \
  --url https://api.devnet.solana.com

# 3. Configure environment credentials
cat <<EOF > .env
SOLANA_RPC_URL=https://api.devnet.solana.com
PROGRAM_ID=$(solana address -k ./target/deploy/univibe-keypair.json)
DISCORD_WEBHOOK_URL=https://api.example.com/webhooks/alerts/placeholder
RESEND_API_KEY=re_your_api_key_here
NOTIFICATION_EMAIL=treasury@univibe.edu
EOF

# 4. Launch turnkey telemetry engine
docker compose up -d --build
docker compose logs -f