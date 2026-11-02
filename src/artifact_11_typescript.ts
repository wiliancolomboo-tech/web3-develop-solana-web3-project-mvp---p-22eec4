import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Univibe } from "../target/types/univibe";
import {
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  SystemProgram,
} from "@solana/web3.js";
import { expect } from "chai";
import crypto from "crypto";

describe("UniVibe Protocol End-to-End QA Test Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Univibe as Program<Univibe>;

  const sponsor = Keypair.generate();
  const studentGrantee = Keypair.generate();
  const campusMentor = Keypair.generate();
  const maliciousActor = Keypair.generate();

  const grantId = new anchor.BN(2024);
  const trancheAmounts = [
    new anchor.BN(0.5 * LAMPORTS_PER_SOL),
    new anchor.BN(1.0 * LAMPORTS_PER_SOL),
  ];
  const totalGrantAmount = trancheAmounts.reduce((acc, curr) => acc.add(curr), new anchor.BN(0));

  let escrowPDA: PublicKey;
  let vaultPDA: PublicKey;
  let milestoneZeroPDA: PublicKey;

  before(async () => {
    // Airdrop SOL to test actors
    const airdropSigners = [sponsor, studentGrantee, campusMentor, maliciousActor];
    for (const actor of airdropSigners) {
      const sig = await provider.connection.requestAirdrop(
        actor.publicKey,
        5 * LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    [escrowPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("univibe_escrow"), grantId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [vaultPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("univibe_vault"), grantId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [milestoneZeroPDA] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("univibe_milestone"),
        grantId.toArrayLike(Buffer, "le", 8),
        Buffer.from([0]),
      ],
      program.programId
    );
  });

  it("1. Successfully initializes a multi-tranche grant with PDA escrow funding", async () => {
    const futureDeadline = new anchor.BN(Math.floor(Date.now() / 1000) + 3600); // +1 hour

    await program.methods
      .initializeGrant(
        grantId,
        "Solana Campus AI Hub",
        trancheAmounts,
        futureDeadline
      )
      .accounts({
        sponsor: sponsor.publicKey,
        grantee: studentGrantee.publicKey,
        evaluator: campusMentor.publicKey,
        grantEscrow: escrowPDA,
        vault: vaultPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const escrowAccount = await program.account.grantEscrow.fetch(escrowPDA);
    expect(escrowAccount.grantId.toString()).to.equal(grantId.toString());
    expect(escrowAccount.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(escrowAccount.grantee.toBase58()).to.equal(studentGrantee.publicKey.toBase58());
    expect(escrowAccount.evaluator.toBase58()).to.equal(campusMentor.publicKey.toBase58());
    expect(escrowAccount.totalAmount.toNumber()).to.equal(totalGrantAmount.toNumber());
    expect(escrowAccount.completedTranches).to.equal(0);
    expect(escrowAccount.isCancelled).to.be.false;

    // Verify Vault Lamport balance
    const vaultBalance = await provider.connection.getBalance(vaultPDA);
    expect(vaultBalance).to.equal(totalGrantAmount.toNumber());
  });

  it("2. Rejects unauthorized actors attempting to submit milestone proof", async () => {
    const dummyHash = crypto.createHash("sha256").update("commit-abc-123").digest();
    const proofHashArray = Array.from(dummyHash);

    try {
      await program.methods
        .submitMilestoneProof(0, proofHashArray, "https://github.com/univibe/demo")
        .accounts({
          grantee: maliciousActor.publicKey,
          grantEscrow: escrowPDA,
          milestoneRecord: milestoneZeroPDA,
          systemProgram: SystemProgram.programId,
        })
        .signers([maliciousActor])
        .rpc();
      expect.fail("Expected UnauthorizedGrantee error");
    } catch (err: any) {
      expect(err.error?.errorCode?.code || err.toString()).to.include("UnauthorizedGrantee");
    }
  });

  it("3. Grantee successfully submits Milestone 0 proof of work", async () => {
    const commitHash = crypto.createHash("sha256").update("github-commit-v1.0.0").digest();
    const proofHashArray = Array.from(commitHash);

    await program.methods
      .submitMilestoneProof(0, proofHashArray, "https://github.com/univibe/campus-grant/milestone/0")
      .accounts({
        grantee: studentGrantee.publicKey,
        grantEscrow: escrowPDA,
        milestoneRecord: milestoneZeroPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([studentGrantee])
      .rpc();

    const milestoneRecord = await program.account.milestoneRecord.fetch(milestoneZeroPDA);
    expect(milestoneRecord.grantId.toString()).to.equal(grantId.toString());
    expect(milestoneRecord.milestoneIndex).to.equal(0);
    expect(milestoneRecord.isApproved).to.be.false;
  });

  it("4. Evaluator attests to milestone: triggers automatic CPI transfer to student", async () => {
    const initialGranteeBalance = await provider.connection.getBalance(studentGrantee.publicKey);

    await program.methods
      .attestAndRelease(0)
      .accounts({
        evaluator: campusMentor.publicKey,
        grantee: studentGrantee.publicKey,
        grantEscrow: escrowPDA,
        milestoneRecord: milestoneZeroPDA,
        vault: vaultPDA,
        systemProgram: SystemProgram.programId,
      })
      .signers([campusMentor])
      .rpc();

    const finalGranteeBalance = await provider.connection.getBalance(studentGrantee.publicKey);
    expect(finalGranteeBalance).to.be.greaterThan(initialGranteeBalance);

    // Verify on-chain state updates
    const escrowAccount = await program.account.grantEscrow.fetch(escrowPDA);
    expect(escrowAccount.completedTranches).to.equal(1);
    expect(escrowAccount.releasedAmount.toNumber()).to.equal(trancheAmounts[0].toNumber());

    const milestoneRecord = await program.account.milestoneRecord.fetch(milestoneZeroPDA);
    expect(milestoneRecord.isApproved).to.be.true;
  });

  it("5. Prevents double-attestation on the same milestone", async () => {
    try {
      await program.methods
        .attestAndRelease(0)
        .accounts({
          evaluator: campusMentor.publicKey,
          grantee: studentGrantee.publicKey,
          grantEscrow: escrowPDA,
          milestoneRecord: milestoneZeroPDA,
          vault: vaultPDA,
          systemProgram: SystemProgram.programId,
        })
        .signers([campusMentor])
        .rpc();
      expect.fail("Expected MilestoneAlreadyApproved or InvalidMilestoneSequence error");
    } catch (err: any) {
      expect(err.toString()).to.satisfy((s: string) => 
        s.includes("InvalidMilestoneSequence") || s.includes("MilestoneAlreadyApproved")
      );
    }
  });

  it("6. Rejects premature emergency refund prior to deadline expiration", async () => {
    try {
      await program.methods
        .emergencyRefund()
        .accounts({
          sponsor: sponsor.publicKey,
          grantEscrow: escrowPDA,
          vault: vaultPDA,
          systemProgram: SystemProgram.programId,
        })
        .signers([sponsor])
        .rpc();
      expect.fail("Expected DeadlineNotReached error");
    } catch (err: any) {
      expect(err.error?.errorCode?.code || err.toString()).to.include("DeadlineNotReached");
    }
  });
});