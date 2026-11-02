use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("UniV1be11111111111111111111111111111111111");

#[program]
pub mod univibe_protocol {
    use super::*;

    pub fn initialize_stream(
        ctx: Context<InitializeStream>,
        stream_id: u64,
        total_amount: u64,
        start_time: i64,
        end_time: i64,
        unlock_bps: Vec<u16>,
    ) -> Result<()> {
        instructions::initialize_stream::handler(ctx, stream_id, total_amount, start_time, end_time, unlock_bps)
    }

    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        proof_uri: String,
    ) -> Result<()> {
        instructions::submit_milestone::handler(ctx, milestone_index, proof_uri)
    }

    pub fn approve_milestone(
        ctx: Context<ApproveMilestone>,
        milestone_index: u8,
    ) -> Result<()> {
        instructions::approve_milestone::handler(ctx, milestone_index)
    }

    pub fn withdraw_streamed_funds(ctx: Context<WithdrawStreamedFunds>) -> Result<()> {
        instructions::withdraw_streamed_funds::handler(ctx)
    }

    pub fn cancel_stream(ctx: Context<CancelStream>) -> Result<()> {
        instructions::cancel_stream::handler(ctx)
    }
}