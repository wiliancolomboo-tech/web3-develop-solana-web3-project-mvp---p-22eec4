import dotenv from "dotenv";
dotenv.config();

export const CONFIG = {
  RPC_URL: process.env.SOLANA_RPC_URL || "http://127.0.0.1:8899",
  WS_URL: process.env.SOLANA_WS_URL || "ws://127.0.0.1:8900",
  PROGRAM_ID: process.env.VIBEGRANT_PROGRAM_ID || "V1beGrant1111111111111111111111111111111111",
  DISCORD_WEBHOOK_URL: process.env.DISCORD_WEBHOOK_URL || "",
  SLACK_WEBHOOK_URL: process.env.SLACK_WEBHOOK_URL || "",
  RESEND_API_KEY: process.env.RESEND_API_KEY || "",
  ALERT_EMAIL_RECIPIENT: process.env.ALERT_EMAIL_RECIPIENT || "faculty@university.edu",
};