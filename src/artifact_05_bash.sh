# 1. Install Anchor CLI and Solana Toolsuite
sh -c "$(curl -sSfL https://release.anza.xyz/stable/install)"
cargo install --git https://github.com/coral-xyz/anchor avm --locked --force
avm install 0.30.1 && avm use 0.30.1

# 2. Configure Solana to Devnet
solana config set --url https://api.devnet.solana.com

# 3. Build & Deploy Smart Contract
git clone https://github.com/vibestream/vibestream-core.git
cd vibestream-core
anchor build
solana program deploy ./target/deploy/vibestream.so

# 4. Copy Program ID to Anchor.toml & Declare_id!
anchor test --skip-local-validator