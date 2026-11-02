import * as anchor from "@coral-xyz/anchor";
import { Connection, PublicKey } from "@solana/web3.js";
import axios from "axios";
import dotenv from "dotenv";

dotenv.config();

interface FundsDisbursedEvent {
  project: PublicKey;
  beneficiary: PublicKey;
  milestoneIndex: number;
  amount: anchor.BN;
  isFullyCompleted: boolean;
}

interface MilestoneAttestedEvent {
  project: PublicKey;
  milestoneIndex: number;
  mentor: PublicKey;
  approved: boolean;
  totalApprovals: number;
  isMilestoneReady: boolean;
}

export class UniVibeRelayer {
  private connection: Connection;
  private resendApiKey: string;
  private discordWebhookUrl: string;
  private programId: PublicKey;

  constructor() {
    const rpcUrl = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
    this.connection = new Connection(rpcUrl, { commitment: "confirmed", wsEndpoint: process.env.SOLANA_WS_URL });
    this.resendApiKey = process.env.RESEND_API_KEY || "";
    this.discordWebhookUrl = process.env.DISCORD_WEBHOOK_URL || "";
    this.programId = new PublicKey(process.env.UNIVIBE_PROGRAM_ID || "UniVibe111111111111111111111111111111111111");

    if (!this.resendApiKey) {
      console.warn("[WARN] RESEND_API_KEY is not defined. Email notifications will run in mock mode.");
    }
  }

  public async sendTransactionalEmail(recipient: string, subject: string, htmlContent: string): Promise<void> {
    if (!this.resendApiKey) {
      console.log(`[MOCK EMAIL] To: ${recipient} | Subject: ${subject}`);
      return;
    }

    try {
      await axios.post(
        "https://api.resend.com/emails",
        {
          from: "UniVibe Alerts <notifications@univibe.network>",
          to: [recipient],
          subject: subject,
          html: htmlContent,
        },
        {
          headers: {
            Authorization: `Bearer ${this.resendApiKey}`,
            "Content-Type": "application/json",
          },
        }
      );
      console.log(`[RESEND SUCCESS] Sent notification to ${recipient}`);
    } catch (err: any) {
      console.error("[RESEND ERROR]", err.response?.data || err.message);
    }
  }

  public async postDiscordBroadcast(title: string, description: string, fields: Array<{ name: string; value: string }>): Promise<void> {
    if (!this.discordWebhookUrl) {
      console.log(`[MOCK DISCORD] ${title}: ${description}`);
      return;
    }

    try {
      await axios.post(this.discordWebhookUrl, {
        username: "UniVibe Campus Oracle",
        avatar_url: "https://cryptologos.cc/logos/solana-sol-logo.png",
        embeds: [
          {
            title,
            description,
            color: 0x14f195, // Solana Green
            fields,
            timestamp: new Date().toISOString(),
          },
        ],
      });
      console.log(`[DISCORD SUCCESS] Broadcast posted`);
    } catch (err: any) {
      console.error("[DISCORD ERROR]", err.response?.data || err.message);
    }
  }

  public async start(): Promise<void> {
    console.log(`[UNIVIBE ORCHESTRATOR] Subscribed to Program: ${this.programId.toBase58()}`);

    // Listen to Solana program logs
    this.connection.onLogs(
      this.programId,
      async (logsContext) => {
        if (logsContext.err) return;

        const logs = logsContext.logs;
        for (const log of logs) {
          if (log.includes("FundsDisbursed")) {
            console.log("[EVENT DETECTED] FundsDisbursed");
            await this.handleFundsDisbursed(log, logsContext.signature);
          } else if (log.includes("MilestoneAttested")) {
            console.log("[EVENT DETECTED] MilestoneAttested");
            await this.handleMilestoneAttested(log, logsContext.signature);
          }
        }
      },
      "confirmed"
    );
  }

  private async handleFundsDisbursed(rawLog: string, txSig: string): Promise<void> {
    const solScanLink = `https://solscan.io/tx/${txSig}?cluster=devnet`;
    const campusAdminEmail = process.env.CAMPUS_ADMIN_EMAIL || "grants-office@university.edu";

    await this.sendTransactionalEmail(
      campusAdminEmail,
      "🎓 Capital Disbursed: UniVibe Milestone Complete",
      `<h2>Milestone Grant Payout Finalized</h2>
       <p>A verified milestone disbursement has executed on-chain.</p>
       <p><strong>Transaction Signature:</strong> <a href="${solScanLink}">${txSig}</a></p>`
    );

    await this.postDiscordBroadcast(
      "🚀 Campus Grant Released!",
      "A student founder has fulfilled their milestone requirements and claimed escrow funds.",
      [
        { name: "Explorer Verification", value: `[View on Solscan](${solScanLink})` },
        { name: "Settlement Speed", value: "Sub-second Finality" },
      ]
    );
  }

  private async handleMilestoneAttested(rawLog: string, txSig: string): Promise<void> {
    await this.postDiscordBroadcast(
      "✍️ Mentor Milestone Attestation Recorded",
      "A faculty evaluator has verified an academic research deliverable on-chain.",
      [{ name: "Tx Proof", value: txSig }]
    );
  }
}

// Turnkey startup
if (require.main === module) {
  const relayer = new UniVibeRelayer();
  relayer.start().catch((err) => {
    console.error("[FATAL RELAYER RUNTIME ERROR]", err);
    process.exit(1);
  });
}