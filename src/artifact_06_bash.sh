# 1. Install dependencies & build contract
npm install
anchor build

# 2. Deploy Anchor Program to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --provider.cluster devnet

# 3. Copy Program ID to Anchor.toml and programs/univibe/src/lib.rs, then rebuild:
anchor build && anchor deploy --provider.cluster devnet

# 4. Run full test matrix
anchor test --skip-local-validator