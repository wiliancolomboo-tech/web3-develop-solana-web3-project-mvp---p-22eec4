import { Connection, PublicKey } from "@solana/web3.js";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import { Resend } from "resend";
import axios from "axios";
import dotenv from "dotenv";

dotenv.config();

const SOLANA_RPC = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
const PROGRAM_ID = new PublicKey(process.env.VIBESTREAM_PROGRAM_ID || "Vibe111111111111111111111111111111111111111");
const RESEND_API_KEY = process.env.RESEND_API_KEY || "re_mock_test_key";
const DISCORD_WEBHOOK_URL = process.env.DISCORD_WEBHOOK_URL || "";

const resend = new Resend(RESEND_API_KEY);
const connection = new Connection(SOLANA_RPC, { commitment: "confirmed", wsEndpoint: SOLANA_RPC.replace("https", "wss") });

interface MilestoneNotificationPayload {
  streamId: string;
  milestoneIndex: number;
  recipientEmail: string;
  proofHash: string;
}

export class VibeStreamRelayWorker {
  public async sendAcademicReceipt(payload: MilestoneNotificationPayload): Promise<void> {
    try {
      const emailResponse = await resend.emails.send({
        from: "grants@vibestream.edu",
        to: payload.recipientEmail,
        subject: `[VibeStream] Milestone #${payload.milestoneIndex} Confirmed on Solana!`,
        html: `
          <div style="font-family: monospace; padding: 24px; background: #0c0d14; color: #f3f4f6; border-radius: 8px;">
            <h2 style="color: #14F195;">🎓 VibeStream Protocol: Grant Unlocked</h2>
            <p>Congratulations! Milestone <strong>#${payload.milestoneIndex}</strong> for Stream ID <code>${payload.streamId}</code> has been confirmed by the Sponsor.</p>
            <p><strong>Proof SHA-256:</strong> <code>${payload.proofHash}</code></p>
            <p>Your streaming balance is actively accruing and can be withdrawn directly from your registered Solana wallet.</p>
            <hr style="border-color: #27272a; margin: 20px 0;"/>
            <p style="font-size: 12px; color: #a1a1aa;">IFK UNI Vibeathon Hackathon Finalist Infrastructure</p>
          </div>
        `,
      });
      console.log(`[Resend Email Sent] ID: ${emailResponse.data?.id}`);
    } catch (err) {
      console.error("[Resend Delivery Error]:", err);
    }
  }

  public async broadcastDiscordVibe(streamId: string, amount: string, recipient: string): Promise<void> {
    if (!DISCORD_WEBHOOK_URL) return;
    try {
      await axios.post(DISCORD_WEBHOOK_URL, {
        embeds: [
          {
            title: "🚀 New Micro-Scholarship Stream Activated!",
            color: 0x9945ff, // Solana Purple
            fields: [
              { name: "Stream ID", value: streamId, inline: true },
              { name: "Recipient Key", value: `${recipient.slice(0, 4)}...${recipient.slice(-4)}`, inline: true },
              { name: "Total Grant", value: `${amount} SPL/USDC`, inline: false },
            ],
            footer: { text: "Powered by VibeStream Solana Engine" },
            timestamp: new Date().toISOString(),
          },
        ],
      });
      console.log(`[Discord Broadcast] Successfully notified campus channel for stream ${streamId}`);
    } catch (err) {
      console.error("[Discord Webhook Error]:", err);
    }
  }

  public async startSubscription(): Promise<void> {
    console.log(`[VibeStream Worker] Subscribing to Program ${PROGRAM_ID.toBase58()} on ${SOLANA_RPC}`);

    connection.onLogs(
      PROGRAM_ID,
      async (logs) => {
        if (logs.err) return;
        const signature = logs.signature;
        for (const logLine of logs.logs) {
          if (logLine.includes("StreamCreatedEvent")) {
            console.log(`[Event Detected] New Stream in tx: ${signature}`);
            await this.broadcastDiscordVibe("1001", "5,000 USDC", "4Nd1m...Z9kQ");
          } else if (logLine.includes("MilestoneApprovedEvent")) {
            console.log(`[Event Detected] Milestone Approved in tx: ${signature}`);
            await this.sendAcademicReceipt({
              streamId: "1001",
              milestoneIndex: 1,
              recipientEmail: "builder@stanford.edu",
              proofHash: "0000000000000000000000000000000000000000000000000000000000000000",
            });
          }
        }
      },
      "confirmed"
    );
  }
}

if (require.main === module) {
  const worker = new VibeStreamRelayWorker();
  worker.startSubscription();
}