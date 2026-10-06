import { Connection, PublicKey } from "@solana/web3.js";
import { Program, AnchorProvider, Idl } from "@coral-xyz/anchor";
import axios from "axios";
import * as dotenv from "dotenv";

dotenv.config();

interface CheckInPayload {
  eventId: string;
  attendee: string;
  claimedAmountSol: number;
  timestamp: number;
  txSignature: string;
}

export class UniVibeRelayerService {
  private connection: Connection;
  private discordWebhookUrl: string;
  private resendApiKey: string;
  private programId: PublicKey;

  constructor() {
    const rpcUrl = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
    this.connection = new Connection(rpcUrl, "confirmed");
    this.discordWebhookUrl = process.env.DISCORD_WEBHOOK_URL || "";
    this.resendApiKey = process.env.RESEND_API_KEY || "";
    this.programId = new PublicKey(
      process.env.PROGRAM_ID || "UniV1bE111111111111111111111111111111111111"
    );

    if (!this.discordWebhookUrl) {
      console.warn("⚠️ Warning: DISCORD_WEBHOOK_URL is not set.");
    }
    if (!this.resendApiKey) {
      console.warn("⚠️ Warning: RESEND_API_KEY is not set.");
    }
  }

  public async startListener(): Promise<void> {
    console.log(`[UniVibe Relayer] Listening for on-chain events on ${this.programId.toBase58()}...`);

    this.connection.onLogs(
      this.programId,
      async ({ logs, err, signature }) => {
        if (err) return;

        // Parse instruction execution and Anchor event markers
        for (const log of logs) {
          if (log.includes("Program log: Instruction: CheckInAndClaim")) {
            console.log(`[CheckIn Detected] TX: ${signature}`);
            
            // Extract event context via parsed transaction
            const parsedTx = await this.connection.getParsedTransaction(signature, {
              maxSupportedTransactionVersion: 0,
            });

            if (!parsedTx || !parsedTx.meta) continue;

            const attendee = parsedTx.transaction.message.accountKeys[2].pubkey.toBase58();
            const payload: CheckInPayload = {
              eventId: "IFK-UNI-VIBE-2025",
              attendee,
              claimedAmountSol: 0.05,
              timestamp: Date.now(),
              txSignature: signature,
            };

            await this.dispatchDiscordNotification(payload);
            await this.dispatchResendEmailReceipt(payload, "student-builder@campus.edu");
          }
        }
      },
      "confirmed"
    );
  }

  private async dispatchDiscordNotification(payload: CheckInPayload): Promise<void> {
    if (!this.discordWebhookUrl) return;

    const embed = {
      title: "🎉 New Campus Vibe Check-In Confirmed!",
      color: 0x14f195, // Solana Emerald
      fields: [
        { name: "Event ID", value: `\`${payload.eventId}\``, inline: true },
        { name: "Attendee Pubkey", value: `\`${payload.attendee.slice(0, 4)}...${payload.attendee.slice(-4)}\``, inline: true },
        { name: "Reward Disbursed", value: `${payload.claimedAmountSol} SOL`, inline: true },
        { name: "Explorer Verification", value: `[View on Solana Explorer](https://explorer.solana.com/tx/${payload.txSignature}?cluster=devnet)` },
      ],
      footer: { text: "UniVibe Protocol • Powered by Solana" },
      timestamp: new Date().toISOString(),
    };

    try {
      await axios.post(this.discordWebhookUrl, { embeds: [embed] });
      console.log(`[Discord] Webhook notification dispatched for ${payload.attendee}`);
    } catch (error: any) {
      console.error(`[Discord Webhook Error]: ${error.message}`);
    }
  }

  private async dispatchResendEmailReceipt(payload: CheckInPayload, recipientEmail: string): Promise<void> {
    if (!this.resendApiKey) return;

    try {
      await axios.post(
        "https://api.resend.com/emails",
        {
          from: "UniVibe Notifications <onboarding@resend.dev>",
          to: [recipientEmail],
          subject: `✨ Proof of Attendance & Grant Receipt: ${payload.eventId}`,
          html: `
            <div style="font-family: Arial, sans-serif; background-color: #0d0e12; color: #ffffff; padding: 24px; border-radius: 8px;">
              <h2 style="color: #14f195;">You're Checked In to ${payload.eventId}!</h2>
              <p>Congratulations! Your cryptographic proof of attendance has been recorded on Solana.</p>
              <div style="background-color: #1a1c24; padding: 16px; border-radius: 6px; margin: 16px 0;">
                <p><strong>Attendee Key:</strong> ${payload.attendee}</p>
                <p><strong>Disbursed Micro-Grant:</strong> ${payload.claimedAmountSol} SOL</p>
                <p><strong>Transaction Signature:</strong> <a style="color: #9945ff;" href="https://explorer.solana.com/tx/${payload.txSignature}?cluster=devnet">${payload.txSignature.slice(0, 16)}...</a></p>
              </div>
              <p style="font-size: 12px; color: #888888;">UniVibe Protocol — IFK UNI Vibeathon MVP</p>
            </div>
          `,
        },
        {
          headers: {
            Authorization: `Bearer ${this.resendApiKey}`,
            "Content-Type": "application/json",
          },
        }
      );
      console.log(`[Resend] Email receipt sent successfully to ${recipientEmail}`);
    } catch (error: any) {
      console.error(`[Resend Error]: ${error.response?.data || error.message}`);
    }
  }
}

// Direct Execution Bootstrap
if (require.main === module) {
  const relayer = new UniVibeRelayerService();
  relayer.startListener().catch((err) => {
    console.error("Fatal Relayer Error:", err);
    process.exit(1);
  });
}