# 1. Ensure Solana CLI is set to Devnet
solana config set --url https://api.devnet.solana.com

# 2. Build the Anchor Program binary
anchor build

# 3. Synchronize program keys
anchor keys sync

# 4. Deploy to Solana Devnet
solana airdrop 2
anchor deploy --provider.cluster devnet

# 5. Export deployed Program ID to .env file
echo "PROGRAM_ID=$(solana address -k target/deploy/campus_vibe-keypair.json)" >> .env
echo "SOLANA_RPC_URL=https://api.devnet.solana.com" >> .env
echo "SOLANA_WS_URL=wss://api.devnet.solana.com" >> .env