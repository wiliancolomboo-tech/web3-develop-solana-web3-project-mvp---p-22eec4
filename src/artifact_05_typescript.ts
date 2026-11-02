import * as dotenv from "dotenv";
dotenv.config();

export interface MilestoneApprovedNotification {
  grantPubkey: string;
  grantee: string;
  milestoneIndex: number;
  payoutSol: number;
  txHash?: string;
  recipientEmail?: string;
}

export class EnterpriseNotifier {
  private resendApiKey: string;
  private discordWebhookUrl: string;

  constructor() {
    this.resendApiKey = process.env.RESEND_API_KEY || "";
    this.discordWebhookUrl = process.env.DISCORD_WEBHOOK_URL || "";
  }

  /**
   * Dispatch transaction receipt via Resend Enterprise Email API
   */
  async sendDisbursementReceipt(data: MilestoneApprovedNotification): Promise<void> {
    if (!this.resendApiKey) {
      console.warn("[EnterpriseNotifier] RESEND_API_KEY not set. Skipping email dispatch.");
      return;
    }

    const targetEmail = data.recipientEmail || process.env.FINANCE_ADMIN_EMAIL || "finance@university.edu";
    const bodyPayload = {
      from: "CampusVibe Protocol <notifications@resend.dev>",
      to: [targetEmail],
      subject: `[DISBURSEMENT NOTICE] Milestone #${data.milestoneIndex + 1} Released: ${data.payoutSol} SOL`,
      html: `
        <div style="font-family: Arial, sans-serif; padding: 20px; border: 1px solid #e0e0e0; border-radius: 8px;">
          <h2 style="color: #4f46e5;">CampusVibe Collegiate Milestone Disbursement</h2>
          <p>This is an automated cryptographic receipt verifying that funds were released on the Solana blockchain.</p>
          <hr style="border: 0; border-top: 1px solid #eeeeee;" />
          <table style="width: 100%; text-align: left;">
            <tr><td><strong>Grant Account:</strong></td><td><code>${data.grantPubkey}</code></td></tr>
            <tr><td><strong>Grantee Recipient:</strong></td><td><code>${data.grantee}</code></td></tr>
            <tr><td><strong>Milestone Approved:</strong></td><td>Tranche index #${data.milestoneIndex}</td></tr>
            <tr><td><strong>Amount Released:</strong></td><td><strong>${data.payoutSol} SOL</strong></td></tr>
            <tr><td><strong>Solana Explorer:</strong></td><td><a href="https://explorer.solana.com/address/${data.grantPubkey}?cluster=devnet">View State on Devnet</a></td></tr>
          </table>
          <hr style="border: 0; border-top: 1px solid #eeeeee;" />
          <p style="font-size: 12px; color: #888888;">Processed via CampusVibe Protocol • IFK UNI Vibeathon Production MVP</p>
        </div>
      `,
    };

    try {
      const response = await fetch("https://api.resend.com/emails", {
        method: "POST",
        headers: {
          "Authorization": `Bearer ${this.resendApiKey}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify(bodyPayload),
      });

      if (!response.ok) {
        const errText = await response.text();
        throw new Error(`Resend API Error HTTP ${response.status}: ${errText}`);
      }

      const resJson = await response.json();
      console.log(`[EnterpriseNotifier] Resend email successfully dispatched. Message ID: ${resJson.id}`);
    } catch (err: any) {
      console.error("[EnterpriseNotifier] Failed to dispatch Resend email:", err.message);
    }
  }

  /**
   * Dispatch instant alert to Campus DAO Discord channel via Webhook
   */
  async broadcastDiscordEvent(title: string, description: string, fields: Array<{ name: string; value: string; inline?: boolean }>): Promise<void> {
    if (!this.discordWebhookUrl) {
      console.warn("[EnterpriseNotifier] DISCORD_WEBHOOK_URL not configured. Skipping webhook.");
      return;
    }

    const payload = {
      username: "CampusVibe Oracle Bot",
      avatar_url: "https://cryptologos.cc/logos/solana-sol-logo.png",
      embeds: [
        {
          title,
          description,
          color: 0x14f195, // Solana Emerald Green
          fields,
          footer: {
            text: "CampusVibe Verifiable Autonomous Settlement Engine",
          },
          timestamp: new Date().toISOString(),
        },
      ],
    };

    try {
      const response = await fetch(this.discordWebhookUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      if (!response.ok) {
        const errText = await response.text();
        throw new Error(`Discord Webhook Error HTTP ${response.status}: ${errText}`);
      }
      console.log(`[EnterpriseNotifier] Discord notification dispatched: "${title}"`);
    } catch (err: any) {
      console.error("[EnterpriseNotifier] Failed to post to Discord webhook:", err.message);
    }
  }
}