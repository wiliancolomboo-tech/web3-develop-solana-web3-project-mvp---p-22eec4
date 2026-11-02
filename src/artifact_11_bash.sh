# 1. Build Solana Program
anchor build

# 2. Extract Program ID and sync to Anchor.toml & lib.rs
solana address -k target/deploy/univibe_core-keypair.json
anchor keys sync

# 3. Deploy to Solana Devnet
solana config set --url devnet
anchor deploy --provider.cluster devnet

# 4. Verify Local Tests
anchor test --skip-deploy

# 5. Launch Enterprise Integration Worker
export DISCORD_WEBHOOK_URL="https://api.example.com/webhooks/alerts/placeholder"
export RESEND_API_KEY="re_123456789"
docker compose up -d --build

# 6. Verify Container Status
docker logs -f univibe_event_engine