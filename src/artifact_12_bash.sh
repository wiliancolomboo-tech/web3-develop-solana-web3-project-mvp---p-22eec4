# 1. Clone repository & install dependencies
git clone https://github.com/univibe-protocol/univibe-core.git
cd univibe-core
npm install -g @coral-xyz/anchor-cli
yarn install

# 2. Build Anchor smart contract and verify program ID
anchor build
solana address -k target/deploy/univibe-keypair.json
# Sync declare_id! in programs/univibe/src/lib.rs with the generated keypair

# 3. Execute test suite locally against local validator
anchor test

# 4. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
anchor deploy --provider.cluster devnet

# 5. Bootstrap Dockerized Enterprise Event Indexer & Discord Alerting
cp .env.example .env
# Edit .env with your RESEND_API_KEY and DISCORD_WEBHOOK_URL
docker compose up -d --build