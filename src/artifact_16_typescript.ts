import { Connection, PublicKey } from "@solana/web3.js";
import { CONFIG } from "./config";
import { broadcastEnterpriseEvent } from "./notifier";

async function runDaemon() {
  console.log("=================================================");
  console.log("  VIBEGRANT PROTOCOL ENTERPRISE EVENT DAEMON     ");
  console.log("=================================================");
  console.log(`Connecting to RPC WebSocket: ${CONFIG.WS_URL}`);
  console.log(`Subscribing to Program: ${CONFIG.PROGRAM_ID}`);

  const connection = new Connection(CONFIG.RPC_URL, {
    wsEndpoint: CONFIG.WS_URL,
    commitment: "confirmed",
  });

  const programPubkey = new PublicKey(CONFIG.PROGRAM_ID);

  connection.onLogs(
    programPubkey,
    async (logInfo, ctx) => {
      const logs = logInfo.logs.join(" ");
      const signature = logInfo.signature;
      console.log(`\n[Tx Detected] Signature: ${signature} (Slot: ${ctx.slot})`);

      if (logs.includes("initialized. Total budget")) {
        await broadcastEnterpriseEvent({
          eventType: "GRANT_INITIALIZED",
          actor: signature,
          details: `New university grant program opened and funded. Signature: ${signature}`,
        });
      } else if (logs.includes("Milestone") && logs.includes("submitted by builder")) {
        await broadcastEnterpriseEvent({
          eventType: "MILESTONE_SUBMITTED",
          actor: signature,
          details: `Builder submitted milestone deliverables for verification. Signature: ${signature}`,
        });
      } else if (logs.includes("approved. Lamports disbursed")) {
        await broadcastEnterpriseEvent({
          eventType: "MILESTONE_APPROVED",
          actor: signature,
          details: `Evaluator signed off on deliverable. Instant settlement disbursed from PDA.`,
        });
      } else if (logs.includes("Clawback executed to sponsor")) {
        await broadcastEnterpriseEvent({
          eventType: "CLAWBACK_EXECUTED",
          actor: signature,
          details: `Grant deadline reached. Unused funds recouped by university treasury.`,
        });
      }
    },
    "confirmed"
  );

  console.log("[Daemon Running] Real-time bridge listening for Solana on-chain runtime events...");
}

runDaemon().catch((err) => {
  console.error("Critical daemon crash:", err);
  process.exit(1);
});