use anchor_lang::prelude::*;
use crate::state::{GrantAccount, MilestoneAccount, MilestoneStatus};
use crate::errors::VibeGrantError;

#[derive(Accounts)]
#[instruction(milestone_id: u8)]
pub struct SubmitMilestone<'info> {
    #[account(mut)]
    pub student: Signer<'info>,

    #[account(
        mut,
        has_one = is_active,
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        init,
        payer = student,
        space = MilestoneAccount::LEN,
        seeds = [b"milestone", grant_account.key().as_ref(), &[milestone_id]],
        bump
    )]
    pub milestone_account: Account<'info, MilestoneAccount>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<SubmitMilestone>,
    milestone_id: u8,
    proof_uri: String,
    amount: u64,
) -> Result<()> {
    require!(proof_uri.len() <= MilestoneAccount::MAXIMUM_URI_LENGTH, VibeGrantError::ProofUriTooLong);
    let grant = &ctx.accounts.grant_account;
    let clock = Clock::get()?;

    require!(clock.unix_timestamp < grant.deadline_epoch, VibeGrantError::GrantExpired);
    require!(amount <= grant.remaining_budget, VibeGrantError::InsufficientGrantFunds);

    let milestone = &mut ctx.accounts.milestone_account;
    milestone.grant = grant.key();
    milestone.student = ctx.accounts.student.key();
    milestone.milestone_id = milestone_id;
    milestone.amount = amount;
    milestone.status = MilestoneStatus::Pending;
    milestone.submitted_at = clock.unix_timestamp;
    milestone.bump = ctx.bumps.milestone_account;
    milestone.proof_uri = proof_uri;

    msg!("Milestone {} submitted by builder {}. Amount: {}", milestone_id, milestone.student, amount);
    Ok(())
}