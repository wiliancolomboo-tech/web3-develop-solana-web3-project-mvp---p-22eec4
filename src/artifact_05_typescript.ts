import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { UniVibe } from "../target/types/univibe";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from "@solana/web3.js";
import { assert, expect } from "chai";

describe("univibe protocol test suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.UniVibe as Program<UniVibe>;
  const organizer = (provider.wallet as anchor.Wallet).payer;

  const attendeeOne = Keypair.generate();
  const attendeeTwo = Keypair.generate();
  const unauthorizedUser = Keypair.generate();

  const eventId = "VIBE-HACK-01";
  const rewardPerAttendee = new anchor.BN(0.1 * LAMPORTS_PER_SOL);
  const maxAttendees = 2;

  let eventVaultPda: PublicKey;
  let vaultBump: number;

  before(async () => {
    // Fund attendee test wallets for rent
    const sig1 = await provider.connection.requestAirdrop(attendeeOne.publicKey, 1 * LAMPORTS_PER_SOL);
    const sig2 = await provider.connection.requestAirdrop(attendeeTwo.publicKey, 1 * LAMPORTS_PER_SOL);
    const sig3 = await provider.connection.requestAirdrop(unauthorizedUser.publicKey, 1 * LAMPORTS_PER_SOL);
    await provider.connection.confirmTransaction(sig1);
    await provider.connection.confirmTransaction(sig2);
    await provider.connection.confirmTransaction(sig3);

    [eventVaultPda, vaultBump] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), organizer.publicKey.toBuffer(), Buffer.from(eventId)],
      program.programId
    );
  });

  it("1. Successfully initializes event vault and locks SOL escrow", async () => {
    const initialOrganizerBalance = await provider.connection.getBalance(organizer.publicKey);

    await program.methods
      .initializeEvent(eventId, rewardPerAttendee, maxAttendees)
      .accounts({
        eventVault: eventVaultPda,
        organizer: organizer.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const vaultAccount = await program.account.eventVault.fetch(eventVaultPda);
    assert.strictEqual(vaultAccount.eventId, eventId);
    assert.strictEqual(vaultAccount.organizer.toBase58(), organizer.publicKey.toBase58());
    assert.strictEqual(vaultAccount.maxAttendees, 2);
    assert.strictEqual(vaultAccount.attendeesCheckedIn, 0);
    assert.isTrue(vaultAccount.isActive);

    const vaultBalance = await provider.connection.getBalance(eventVaultPda);
    assert.isAtLeast(vaultBalance, rewardPerAttendee.toNumber() * maxAttendees);
  });

  it("2. Allows first attendee to check-in and claim micro-grant", async () => {
    const [attendanceRecordPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("attendance"), Buffer.from(eventId), attendeeOne.publicKey.toBuffer()],
      program.programId
    );

    const initialBalance = await provider.connection.getBalance(attendeeOne.publicKey);

    await program.methods
      .checkInAndClaim(eventId)
      .accounts({
        eventVault: eventVaultPda,
        attendanceRecord: attendanceRecordPda,
        attendee: attendeeOne.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([attendeeOne])
      .rpc();

    const finalBalance = await provider.connection.getBalance(attendeeOne.publicKey);
    assert.isAbove(finalBalance, initialBalance);

    const record = await program.account.attendanceRecord.fetch(attendanceRecordPda);
    assert.strictEqual(record.attendee.toBase58(), attendeeOne.publicKey.toBase58());
    assert.strictEqual(record.eventId, eventId);

    const vaultAccount = await program.account.eventVault.fetch(eventVaultPda);
    assert.strictEqual(vaultAccount.attendeesCheckedIn, 1);
  });

  it("3. Strictly prevents duplicate check-in from the same attendee", async () => {
    const [attendanceRecordPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("attendance"), Buffer.from(eventId), attendeeOne.publicKey.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .checkInAndClaim(eventId)
        .accounts({
          eventVault: eventVaultPda,
          attendanceRecord: attendanceRecordPda,
          attendee: attendeeOne.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([attendeeOne])
        .rpc();
      assert.fail("Double check-in should have failed account initialization constraints!");
    } catch (err: any) {
      // Expected Anchor initialization collision error
      expect(err.toString()).to.include("already in use");
    }
  });

  it("4. Prevents unauthorized users from closing the event vault", async () => {
    try {
      await program.methods
        .closeEvent(eventId)
        .accounts({
          eventVault: eventVaultPda,
          organizer: unauthorizedUser.publicKey,
          systemProgram: anchor.web3.SystemProgram.programId,
        })
        .signers([unauthorizedUser])
        .rpc();
      assert.fail("Unauthorized closer should have triggered constraint violation!");
    } catch (err: any) {
      expect(err.toString()).to.include("UnauthorizedOrganizer");
    }
  });

  it("5. Allows organizer to close event and sweep remaining funds", async () => {
    const initialOrganizerBalance = await provider.connection.getBalance(organizer.publicKey);

    await program.methods
      .closeEvent(eventId)
      .accounts({
        eventVault: eventVaultPda,
        organizer: organizer.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    const vaultAccount = await program.account.eventVault.fetch(eventVaultPda);
    assert.isFalse(vaultAccount.isActive);

    const finalOrganizerBalance = await provider.connection.getBalance(organizer.publicKey);
    assert.isAbove(finalOrganizerBalance, initialOrganizerBalance);
  });
});