import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Vibegrant } from "../target/types/vibegrant";
import { Keypair, LAMPORTS_PER_SOL, PublicKey, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";

describe("vibegrant-protocol-tests", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.Vibegrant as Program<Vibegrant>;

  const sponsor = Keypair.generate();
  const reviewer = Keypair.generate();
  const student = Keypair.generate();
  const maliciousActor = Keypair.generate();

  const grantId = new anchor.BN(9001);
  const grantBudget = new anchor.BN(2 * LAMPORTS_PER_SOL);

  let grantPda: PublicKey;
  let vaultPda: PublicKey;
  let milestonePda: PublicKey;

  before(async () => {
    // Airdrop SOL to test actors
    for (const actor of [sponsor, reviewer, student, maliciousActor]) {
      const sig = await provider.connection.requestAirdrop(actor.publicKey, 10 * LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig, "confirmed");
    }

    [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), sponsor.publicKey.toBuffer(), grantId.toArrayLike(Buffer, "le", 8)],
      program.programId
    );

    [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), grantPda.toBuffer()],
      program.programId
    );

    [milestonePda] = PublicKey.findProgramAddressSync(
      [Buffer.from("milestone"), grantPda.toBuffer(), Buffer.from([1])],
      program.programId
    );
  });

  it("1. Successfully initializes grant and transfers lamports into PDA vault", async () => {
    const deadline = new anchor.BN(Math.floor(Date.now() / 1000) + 3600); // 1 hour ahead

    await program.methods
      .initializeGrant(grantId, "IFK UNI Vibeathon Micro-Grant", grantBudget, deadline)
      .accounts({
        sponsor: sponsor.publicKey,
        reviewer: reviewer.publicKey,
        grantAccount: grantPda,
        vault: vaultPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([sponsor])
      .rpc();

    const grantAccount = await program.account.grantAccount.fetch(grantPda);
    expect(grantAccount.sponsor.toBase58()).to.equal(sponsor.publicKey.toBase58());
    expect(grantAccount.reviewer.toBase58()).to.equal(reviewer.publicKey.toBase58());
    expect(grantAccount.totalBudget.toNumber()).to.equal(grantBudget.toNumber());
    expect(grantAccount.remainingBudget.toNumber()).to.equal(grantBudget.toNumber());

    const vaultBalance = await provider.connection.getBalance(vaultPda);
    expect(vaultBalance).to.equal(grantBudget.toNumber());
  });

  it("2. Permits student builder to submit valid milestone deliverable", async () => {
    const requestedAmount = new anchor.BN(1 * LAMPORTS_PER_SOL);
    const proofUri = "https://ipfs.io/ipfs/bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi";

    await program.methods
      .submitMilestone(1, proofUri, requestedAmount)
      .accounts({
        student: student.publicKey,
        grantAccount: grantPda,
        milestoneAccount: milestonePda,
        systemProgram: SystemProgram.programId,
      })
      .signers([student])
      .rpc();

    const milestoneAccount = await program.account.milestoneAccount.fetch(milestonePda);
    expect(milestoneAccount.student.toBase58()).to.equal(student.publicKey.toBase58());
    expect(milestoneAccount.amount.toNumber()).to.equal(requestedAmount.toNumber());
    expect(milestoneAccount.milestoneId).to.equal(1);
    expect(milestoneAccount.status).to.deep.equal({ pending: {} });
  });

  it("3. Blocks unauthorized reviewer from approving milestone deliverable", async () => {
    try {
      await program.methods
        .approveMilestone()
        .accounts({
          reviewer: maliciousActor.publicKey,
          student: student.publicKey,
          grantAccount: grantPda,
          milestoneAccount: milestonePda,
          vault: vaultPda,
          systemProgram: SystemProgram.programId,
        })
        .signers([maliciousActor])
        .rpc();

      expect.fail("Unauthorized approval should have thrown an error.");
    } catch (err: any) {
      expect(err.toString()).to.include("UnauthorizedReviewer");
    }
  });

  it("4. Reviewer signs off and vault autonomously disburses SOL to student", async () => {
    const studentPreBalance = await provider.connection.getBalance(student.publicKey);

    await program.methods
      .approveMilestone()
      .accounts({
        reviewer: reviewer.publicKey,
        student: student.publicKey,
        grantAccount: grantPda,
        milestoneAccount: milestonePda,
        vault: vaultPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([reviewer])
      .rpc();

    const milestoneAccount = await program.account.milestoneAccount.fetch(milestonePda);
    expect(milestoneAccount.status).to.deep.equal({ approved: {} });

    const grantAccount = await program.account.grantAccount.fetch(grantPda);
    expect(grantAccount.remainingBudget.toNumber()).to.equal(1 * LAMPORTS_PER_SOL);

    const studentPostBalance = await provider.connection.getBalance(student.publicKey);
    expect(studentPostBalance - studentPreBalance).to.equal(1 * LAMPORTS_PER_SOL);
  });

  it("5. Prevents double-spending already approved milestone", async () => {
    try {
      await program.methods
        .approveMilestone()
        .accounts({
          reviewer: reviewer.publicKey,
          student: student.publicKey,
          grantAccount: grantPda,
          milestoneAccount: milestonePda,
          vault: vaultPda,
          systemProgram: SystemProgram.programId,
        })
        .signers([reviewer])
        .rpc();

      expect.fail("Re-approving an approved milestone should have failed.");
    } catch (err: any) {
      expect(err.toString()).to.include("MilestoneAlreadyProcessed");
    }
  });
});