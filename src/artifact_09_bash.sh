# 1. Build & verify Anchor on-chain program
anchor build

# 2. Deploy to Solana Devnet
solana config set --url devnet
solana airdrop 2
anchor deploy --provider.cluster devnet

# 3. Execute deterministic TypeScript test suite
anchor test

# 4. Spin up Off-Chain Automation Service in Docker
docker-compose up -d --build

# 5. Monitor real-time logs
docker logs -f univibe-automation-relayer