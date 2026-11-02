# 1. Clone repository and install dependencies
git clone https://github.com/univibe-protocol/univibe-core.git
cd univibe-core
npm install

# 2. Build Anchor smart contract
anchor build

# 3. Synchronize program keys
solana address -k target/deploy/univibe_protocol-keypair.json
# Ensure this matches declare_id!() in lib.rs

# 4. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --provider.cluster devnet

# 5. Seed environment variables for automation worker
cp .env.example .env
# Fill in DISCORD_WEBHOOK_URL and RESEND_API_KEY