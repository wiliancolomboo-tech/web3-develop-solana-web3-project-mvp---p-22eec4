import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";

describe("campus_vibe protocol test matrix", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  // We compile against the local program workspace
  const program = anchor.workspace.CampusVibe as Program<any>;

  const sponsor = Keypair.generate();
  const grantee = Keypair.generate();
  const reviewer = Keypair.generate();
  const maliciousActor = Keypair.generate();

  const grantId = "IFK-UNI-HACK-01";
  const title = "Autonomous Solana Agent for Campus";
  const totalAmount = new anchor.BN(2 * LAMPORTS_PER_SOL);
  const totalMilestones = 2;

  let grantPda: PublicKey;
  let vaultPda: PublicKey;
  let grantBump: number;
  let vaultBump: number;

  before(async () => {
    // Airdrop funds to all test accounts
    const airdropSigners = [sponsor, grantee, reviewer, maliciousActor];
    for (const kp of airdropSigners) {
      const sig = await provider.connection.requestAirdrop(kp.publicKey, 5 * LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    [grantPda, grantBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.publicKey.toBuffer(), Buffer.from(grantId)],
      program.programId
    );

    [vaultPda, vaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), grantPda.toBuffer()],
      program.programId
    );
  });

  it("QA-01: Successfully initializes a grant and locks funds in Vault PDA", async () => {
    const initialSponsorBal = await provider.connection.getBalance(sponsor.publicKey);

    await program.methods
      .initializeGrant(grantId, title, totalMilestones, totalAmount)
      .accounts({
        grant: grantPda,
        vault: vaultPda,
        sponsor: sponsor.publicKey,
        grantee: grantee.publicKey,
        reviewer: reviewer.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const grantAccount = await program.account.grantAccount.fetch(grantPda);
    expect(grantAccount.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(grantAccount.grantee.toBase58()).to.equal(grantee.publicKey.toBase58());
    expect(grantAccount.reviewer.toBase58()).to.equal(reviewer.publicKey.toBase58());
    expect(grantAccount.totalAmount.toNumber()).to.equal(totalAmount.toNumber());
    expect(grantAccount.currentMilestone).to.equal(0);
    expect(grantAccount.isActive).to.be.true;
    expect(grantAccount.isCompleted).to.be.false;

    const vaultBalance = await provider.connection.getBalance(vaultPda);
    expect(vaultBalance).to.equal(totalAmount.toNumber());
  });

  it("QA-02: Grantee successfully submits Milestone 0 deliverable", async () => {
    const milestoneIndex = 0;
    const proofUri = "https://github.com/ifk-uni/campus-vibe-submission";

    const [submissionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("submission"), grantPda.toBuffer(), Buffer.from([milestoneIndex])],
      program.programId
    );

    await program.methods
      .submitMilestone(milestoneIndex, proofUri)
      .accounts({
        grant: grantPda,
        submission: submissionPda,
        grantee: grantee.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([grantee])
      .rpc();

    const submissionAccount = await program.account.milestoneSubmission.fetch(submissionPda);
    expect(submissionAccount.grant.toBase58()).to.equal(grantPda.toBase58());
    expect(submissionAccount.milestoneIndex).to.equal(0);
    expect(submissionAccount.proofUri).to.equal(proofUri);
    expect(submissionAccount.isApproved).to.be.false;
  });

  it("QA-03: Malicious actor cannot approve milestone (Security Check)", async () => {
    const milestoneIndex = 0;
    const [submissionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("submission"), grantPda.toBuffer(), Buffer.from([milestoneIndex])],
      program.programId
    );

    try {
      await program.methods
        .evaluateMilestone(milestoneIndex, true)
        .accounts({
          grant: grantPda,
          submission: submissionPda,
          vault: vaultPda,
          reviewer: maliciousActor.publicKey,
          grantee: grantee.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([maliciousActor])
        .rpc();

      expect.fail("Should have rejected evaluation by unauthorized non-reviewer");
    } catch (err: any) {
      expect(err.toString()).to.include("UnauthorizedReviewer");
    }
  });

  it("QA-04: Reviewer approves Milestone 0 and triggers atomic payout to Grantee", async () => {
    const milestoneIndex = 0;
    const [submissionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("submission"), grantPda.toBuffer(), Buffer.from([milestoneIndex])],
      program.programId
    );

    const granteeBalBefore = await provider.connection.getBalance(grantee.publicKey);

    await program.methods
      .evaluateMilestone(milestoneIndex, true)
      .accounts({
        grant: grantPda,
        submission: submissionPda,
        vault: vaultPda,
        reviewer: reviewer.publicKey,
        grantee: grantee.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([reviewer])
      .rpc();

    const granteeBalAfter = await provider.connection.getBalance(grantee.publicKey);
    const expectedTranche = 1 * LAMPORTS_PER_SOL;

    // Grantee balance should have increased by exactly 1 SOL
    expect(granteeBalAfter - granteeBalBefore).to.equal(expectedTranche);

    const grantAccount = await program.account.grantAccount.fetch(grantPda);
    expect(grantAccount.currentMilestone).to.equal(1);
    expect(grantAccount.amountDisbursed.toNumber()).to.equal(expectedTranche);
  });

  it("QA-05: Reviewer cannot re-approve an already paid milestone (Double-spend Check)", async () => {
    const milestoneIndex = 0;
    const [submissionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("submission"), grantPda.toBuffer(), Buffer.from([milestoneIndex])],
      program.programId
    );

    try {
      await program.methods
        .evaluateMilestone(milestoneIndex, true)
        .accounts({
          grant: grantPda,
          submission: submissionPda,
          vault: vaultPda,
          reviewer: reviewer.publicKey,
          grantee: grantee.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([reviewer])
        .rpc();

      expect.fail("Should have failed to evaluate already paid milestone");
    } catch (err: any) {
      expect(err.toString()).to.include("InvalidMilestoneIndex");
    }
  });

  it("QA-06: Grantee submits and receives final milestone, completing the grant", async () => {
    const milestoneIndex = 1;
    const [submissionPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("submission"), grantPda.toBuffer(), Buffer.from([milestoneIndex])],
      program.programId
    );

    await program.methods
      .submitMilestone(milestoneIndex, "https://github.com/ifk-uni/final-mvp-release")
      .accounts({
        grant: grantPda,
        submission: submissionPda,
        grantee: grantee.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([grantee])
      .rpc();

    await program.methods
      .evaluateMilestone(milestoneIndex, true)
      .accounts({
        grant: grantPda,
        submission: submissionPda,
        vault: vaultPda,
        reviewer: reviewer.publicKey,
        grantee: grantee.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([reviewer])
      .rpc();

    const grantAccount = await program.account.grantAccount.fetch(grantPda);
    expect(grantAccount.isCompleted).to.be.true;
    expect(grantAccount.isActive).to.be.false;
    expect(grantAccount.amountDisbursed.toNumber()).to.equal(totalAmount.toNumber());

    const vaultBalance = await provider.connection.getBalance(vaultPda);
    expect(vaultBalance).to.equal(0);
  });
});