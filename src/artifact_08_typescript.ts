import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { UnivibeCore } from "../target/types/univibe_core";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";
import * as crypto from "crypto";

describe("univibe_core end-to-end integration tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.UnivibeCore as Program<UnivibeCore>;

  const sponsor = Keypair.generate();
  const recipient = Keypair.generate();
  const evaluator = Keypair.generate();
  const attacker = Keypair.generate();

  const grantId = `grant-${Date.now()}`;
  const title = "Autonomous Uni Sub-DAO MVP";
  const totalMilestones = 2;
  const targetLamports = new anchor.BN(2 * LAMPORTS_PER_SOL);

  let grantVaultPda: PublicKey;
  let escrowWalletPda: PublicKey;

  before(async () => {
    // Airdrop funds to actors
    const airdropSignatures = await Promise.all([
      provider.connection.requestAirdrop(sponsor.publicKey, 10 * LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(recipient.publicKey, 2 * LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(evaluator.publicKey, 2 * LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(attacker.publicKey, 2 * LAMPORTS_PER_SOL),
    ]);

    for (const sig of airdropSignatures) {
      const latestBlockhash = await provider.connection.getLatestBlockhash();
      await provider.connection.confirmTransaction({
        signature: sig,
        blockhash: latestBlockhash.blockhash,
        lastValidBlockHeight: latestBlockhash.lastValidBlockHeight,
      });
    }

    [grantVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), Buffer.from(grantId)],
      program.programId
    );

    [escrowWalletPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), Buffer.from(grantId)],
      program.programId
    );
  });

  it("1. Successfully initializes a grant vault with deterministic PDAs", async () => {
    await program.methods
      .initializeGrantVault(grantId, title, totalMilestones, targetLamports)
      .accounts({
        grantVault: grantVaultPda,
        escrowWallet: escrowWalletPda,
        sponsor: sponsor.publicKey,
        recipient: recipient.publicKey,
        evaluator: evaluator.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const vaultAccount = await program.account.grantVault.fetch(grantVaultPda);
    expect(vaultAccount.grantId).to.equal(grantId);
    expect(vaultAccount.title).to.equal(title);
    expect(vaultAccount.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(vaultAccount.recipient.toBase58()).to.equal(recipient.publicKey.toBase58());
    expect(vaultAccount.evaluator.toBase58()).to.equal(evaluator.publicKey.toBase58());
    expect(vaultAccount.totalMilestones).to.equal(2);
    expect(vaultAccount.currentMilestone).to.equal(0);
    expect(vaultAccount.totalTargetLamports.toNumber()).to.equal(2 * LAMPORTS_PER_SOL);
  });

  it("2. Funds the grant vault via PDA Lamport transfer", async () => {
    const fundAmount = new anchor.BN(2 * LAMPORTS_PER_SOL);

    await program.methods
      .fundGrant(fundAmount)
      .accounts({
        grantVault: grantVaultPda,
        escrowWallet: escrowWalletPda,
        sponsor: sponsor.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const vaultAccount = await program.account.grantVault.fetch(grantVaultPda);
    expect(vaultAccount.fundedLamports.toNumber()).to.equal(2 * LAMPORTS_PER_SOL);

    const escrowBalance = await provider.connection.getBalance(escrowWalletPda);
    expect(escrowBalance).to.be.at.least(2 * LAMPORTS_PER_SOL);
  });

  it("3. Prevents unauthorized submission of milestone deliverable", async () => {
    const rawHash = crypto.createHash("sha256").update("proof-1").digest();
    const hashArray = Array.from(rawHash);

    try {
      await program.methods
        .submitMilestone(1, hashArray)
        .accounts({
          grantVault: grantVaultPda,
          recipient: attacker.publicKey,
        })
        .signers([attacker])
        .rpc();
      expect.fail("Should have thrown UnauthorizedRecipient error");
    } catch (err: any) {
      expect(err.toString()).to.include("UnauthorizedRecipient");
    }
  });

  it("4. Recipient successfully submits Milestone #1 deliverable", async () => {
    const rawHash = crypto.createHash("sha256").update("proof-milestone-1").digest();
    const hashArray = Array.from(rawHash);

    await program.methods
      .submitMilestone(1, hashArray)
      .accounts({
        grantVault: grantVaultPda,
        recipient: recipient.publicKey,
      })
      .signers([recipient])
      .rpc();

    const vaultAccount = await program.account.grantVault.fetch(grantVaultPda);
    expect(vaultAccount.deliverableHash).to.deep.equal(hashArray);
  });

  it("5. Evaluator approves Milestone #1 and triggers atomic 50% tranche disburse", async () => {
    const recipientBalanceBefore = await provider.connection.getBalance(recipient.publicKey);

    await program.methods
      .approveAndDisburse()
      .accounts({
        grantVault: grantVaultPda,
        escrowWallet: escrowWalletPda,
        evaluator: evaluator.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([evaluator])
      .rpc();

    const recipientBalanceAfter = await provider.connection.getBalance(recipient.publicKey);
    const vaultAccount = await program.account.grantVault.fetch(grantVaultPda);

    expect(vaultAccount.currentMilestone).to.equal(1);
    expect(vaultAccount.disbursedLamports.toNumber()).to.equal(1 * LAMPORTS_PER_SOL);
    expect(recipientBalanceAfter - recipientBalanceBefore).to.equal(1 * LAMPORTS_PER_SOL);
  });

  it("6. Submits and approves Milestone #2 (completing grant)", async () => {
    const rawHash = crypto.createHash("sha256").update("final-milestone-proof").digest();
    const hashArray = Array.from(rawHash);

    await program.methods
      .submitMilestone(2, hashArray)
      .accounts({
        grantVault: grantVaultPda,
        recipient: recipient.publicKey,
      })
      .signers([recipient])
      .rpc();

    await program.methods
      .approveAndDisburse()
      .accounts({
        grantVault: grantVaultPda,
        escrowWallet: escrowWalletPda,
        evaluator: evaluator.publicKey,
        recipient: recipient.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([evaluator])
      .rpc();

    const vaultAccount = await program.account.grantVault.fetch(grantVaultPda);
    expect(vaultAccount.currentMilestone).to.equal(2);
    expect(vaultAccount.disbursedLamports.toNumber()).to.equal(2 * LAMPORTS_PER_SOL);
  });
});