import { Connection, PublicKey } from "@solana/web3.js";
import { Program, AnchorProvider, Idl, EventParser, BorshCoder } from "@coral-xyz/anchor";
import { EnterpriseNotifier } from "./notifier";
import * as dotenv from "dotenv";
dotenv.config();

const PROGRAM_ID = new PublicKey(
  process.env.PROGRAM_ID || "CampVibe111111111111111111111111111111111111"
);

// Simplified inline IDL snippet for production event parsing
const IDL_EVENTS: Idl = {
  version: "0.1.0",
  name: "campus_vibe",
  instructions: [],
  events: [
    {
      name: "GrantCreatedEvent",
      fields: [
        { name: "grantPubkey", type: "publicKey", index: false },
        { name: "sponsor", type: "publicKey", index: false },
        { name: "grantee", type: "publicKey", index: false },
        { name: "grantId", type: "string", index: false },
        { name: "totalAmount", type: "u64", index: false },
        { name: "totalMilestones", type: "u8", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
    {
      name: "MilestoneSubmittedEvent",
      fields: [
        { name: "grantPubkey", type: "publicKey", index: false },
        { name: "grantee", type: "publicKey", index: false },
        { name: "milestoneIndex", type: "u8", index: false },
        { name: "proofUri", type: "string", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
    {
      name: "MilestoneEvaluatedEvent",
      fields: [
        { name: "grantPubkey", type: "publicKey", index: false },
        { name: "milestoneIndex", type: "u8", index: false },
        { name: "approved", type: "bool", index: false },
        { name: "payoutAmount", type: "u64", index: false },
        { name: "recipient", type: "publicKey", index: false },
        { name: "timestamp", type: "i64", index: false },
      ],
    },
  ],
};

async function runRelayer() {
  const rpcUrl = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
  const wsUrl = process.env.SOLANA_WS_URL || "wss://api.devnet.solana.com";

  console.log(`[Relayer Engine] Initializing CampusVibe Relayer on ${rpcUrl}...`);
  const connection = new Connection(rpcUrl, {
    wsEndpoint: wsUrl,
    commitment: "confirmed",
  });

  const notifier = new EnterpriseNotifier();
  const coder = new BorshCoder(IDL_EVENTS);
  const eventParser = new EventParser(PROGRAM_ID, coder);

  console.log(`[Relayer Engine] Subscribing to Program Logs for: ${PROGRAM_ID.toBase58()}`);

  connection.onLogs(
    PROGRAM_ID,
    async ({ logs, err, signature }) => {
      if (err) return;

      const parsedEvents = eventParser.parseLogs(logs);
      for (const event of parsedEvents) {
        console.log(`[Event Received] ${event.name}:`, event.data);

        if (event.name === "GrantCreatedEvent") {
          const d = event.data as any;
          await notifier.broadcastDiscordEvent(
            "🎓 New Campus Grant Escrow Seeded!",
            `Grant **${d.grantId}** has been initialized with funded escrow on Solana.`,
            [
              { name: "Sponsor", value: d.sponsor.toString(), inline: true },
              { name: "Grantee", value: d.grantee.toString(), inline: true },
              { name: "Total Escrow", value: `${(d.totalAmount.toNumber() / 1e9).toFixed(3)} SOL`, inline: true },
              { name: "Milestones", value: `${d.totalMilestones}`, inline: true },
              { name: "Signature", value: `[Solana Explorer](https://explorer.solana.com/tx/${signature}?cluster=devnet)` },
            ]
          );
        } else if (event.name === "MilestoneSubmittedEvent") {
          const d = event.data as any;
          await notifier.broadcastDiscordEvent(
            "📝 Milestone Deliverable Submitted",
            `A grantee has submitted work for Milestone Index **#${d.milestoneIndex}**.`,
            [
              { name: "Grant Account", value: d.grantPubkey.toString() },
              { name: "Proof URL", value: d.proofUri },
              { name: "Submission Time", value: new Date(d.timestamp.toNumber() * 1000).toUTCString() },
            ]
          );
        } else if (event.name === "MilestoneEvaluatedEvent") {
          const d = event.data as any;
          const payoutSol = d.payoutAmount.toNumber() / 1e9;

          if (d.approved) {
            await notifier.broadcastDiscordEvent(
              "✅ Milestone Approved & Payout Disbursed!",
              `Milestone **#${d.milestoneIndex}** verified. Funds atomically transferred from Vault PDA to student.`,
              [
                { name: "Recipient", value: d.recipient.toString(), inline: true },
                { name: "Disbursed", value: `${payoutSol} SOL`, inline: true },
                { name: "Tx Receipt", value: signature },
              ]
            );

            await notifier.sendDisbursementReceipt({
              grantPubkey: d.grantPubkey.toString(),
              grantee: d.recipient.toString(),
              milestoneIndex: d.milestoneIndex,
              payoutSol,
              txHash: signature,
            });
          }
        }
      }
    },
    "confirmed"
  );

  console.log("[Relayer Engine] Daemon operational. Awaiting on-chain slots...");
}

runRelayer().catch((e) => {
  console.error("[Relayer Engine Fatal Error]:", e);
  process.exit(1);
});