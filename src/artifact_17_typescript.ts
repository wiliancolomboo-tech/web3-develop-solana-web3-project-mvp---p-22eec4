import { Command } from "commander";
import { Connection, Keypair, PublicKey, SystemProgram, LAMPORTS_PER_SOL } from "@solana/web3.js";
import * as anchor from "@coral-xyz/anchor";
import fs from "fs";
import { CONFIG } from "./config";

const program = new Command();
program.name("vibegrant").description("VibeGrant Protocol Operator CLI").version("1.0.0");

function loadKeypair(path: string): Keypair {
  const raw = fs.readFileSync(path, "utf-8");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(raw)));
}

program
  .command("init")
  .description("Initialize a new academic grant vault")
  .requiredOption("-k, --keypair <path>", "Path to Sponsor Keypair JSON")
  .requiredOption("-r, --reviewer <pubkey>", "Public Key of the Designated Reviewer")
  .requiredOption("-i, --grant-id <number>", "Unique numeric Grant ID")
  .requiredOption("-t, --title <string>", "Title of the grant")
  .requiredOption("-b, --budget <number>", "Total budget in SOL")
  .requiredOption("-d, --days <number>", "Duration in days")
  .action(async (options) => {
    const sponsor = loadKeypair(options.keypair);
    const connection = new Connection(CONFIG.RPC_URL, "confirmed");
    const reviewerPubkey = new PublicKey(options.reviewer);
    const grantId = new anchor.BN(options.grantId);
    const budgetLamports = new anchor.BN(parseFloat(options.budget) * LAMPORTS_PER_SOL);
    const deadline = new anchor.BN(Math.floor(Date.now() / 1000) + parseInt(options.days) * 86400);

    const programId = new PublicKey(CONFIG.PROGRAM_ID);
    const [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.publicKey.toBuffer(), grantId.toArrayLike(Buffer, "le", 8)],
      programId
    );
    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), grantPda.toBuffer()],
      programId
    );

    console.log(`Initializing Grant ${options.grantId}...`);
    console.log(`Grant PDA: ${grantPda.toBase58()}`);
    console.log(`Vault PDA: ${vaultPda.toBase58()}`);
    console.log(`Budget: ${options.budget} SOL`);
  });

program.parse(process.argv);