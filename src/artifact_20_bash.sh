# Configure CLI target
solana config set --url https://api.devnet.solana.com

# Deploy Program
solana program deploy ./target/deploy/vibegrant.so \
  --program-id ./target/deploy/vibegrant-keypair.json \
  --upgrade-authority ~/.config/solana/id.json