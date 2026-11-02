# 1. Clone repository & install dependencies
git clone https://github.com/univibe-solana/univibe-core.git
cd univibe-core

# 2. Build on-chain program
anchor build

# 3. Synchronize program keys
solana address -k target/deploy/univibe-keypair.json
# (Paste output into declare_id! in programs/univibe/src/lib.rs if re-keying)

# 4. Deploy to Solana Devnet
solana config set --url devnet
solana airdrop 2
anchor deploy --provider.cluster devnet

# 5. Launch Enterprise Background Daemon
cd client
cp .env.example .env
npm install
npm run build
npm start