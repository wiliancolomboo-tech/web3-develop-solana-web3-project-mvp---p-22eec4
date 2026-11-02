import axios from "axios";
import { CONFIG } from "./config";

export interface GrantNotificationPayload {
  eventType: "GRANT_INITIALIZED" | "MILESTONE_SUBMITTED" | "MILESTONE_APPROVED" | "CLAWBACK_EXECUTED";
  grantPubkey?: string;
  milestoneId?: number;
  amountLamports?: number;
  actor: string;
  details: string;
}

export async function broadcastEnterpriseEvent(payload: GrantNotificationPayload): Promise<void> {
  const message = `[VibeGrant Protocol Alert]
Event: ${payload.eventType}
Actor: ${payload.actor}
Amount (SOL): ${payload.amountLamports ? (payload.amountLamports / 1e9).toFixed(4) : "N/A"}
Details: ${payload.details}
Timestamp: ${new Date().toISOString()}`;

  // 1. Post to Discord Webhook
  if (CONFIG.DISCORD_WEBHOOK_URL) {
    try {
      await axios.post(CONFIG.DISCORD_WEBHOOK_URL, {
        content: `**[VibeGrant Security Engine]** \`${payload.eventType}\`\n>>> **Actor:** \`${payload.actor}\`\n**Details:** ${payload.details}`,
      });
      console.log("[Notifier] Discord webhook dispatched.");
    } catch (err: any) {
      console.error("[Notifier] Failed to send Discord notification:", err.message);
    }
  }

  // 2. Post to Slack Webhook
  if (CONFIG.SLACK_WEBHOOK_URL) {
    try {
      await axios.post(CONFIG.SLACK_WEBHOOK_URL, {
        text: message,
      });
      console.log("[Notifier] Slack notification dispatched.");
    } catch (err: any) {
      console.error("[Notifier] Failed to send Slack notification:", err.message);
    }
  }

  // 3. Post to Resend Transactional Email Engine
  if (CONFIG.RESEND_API_KEY && CONFIG.ALERT_EMAIL_RECIPIENT) {
    try {
      await axios.post(
        "https://api.resend.com/emails",
        {
          from: "alerts@vibegrant.network",
          to: [CONFIG.ALERT_EMAIL_RECIPIENT],
          subject: `[VibeGrant Protocol] Action Alert: ${payload.eventType}`,
          text: message,
        },
        {
          headers: {
            Authorization: `Bearer ${CONFIG.RESEND_API_KEY}`,
            "Content-Type": "application/json",
          },
        }
      );
      console.log("[Notifier] Enterprise receipt dispatched via Resend.");
    } catch (err: any) {
      console.error("[Notifier] Failed to dispatch Resend email:", err.response?.data || err.message);
    }
  }
}