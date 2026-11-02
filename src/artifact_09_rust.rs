use anchor_lang::prelude::*;
use crate::state::{GrantAccount, MilestoneAccount, MilestoneStatus};
use crate::errors::VibeGrantError;

#[derive(Accounts)]
pub struct ApproveMilestone<'info> {
    #[account(
        mut,
        address = grant_account.reviewer @ VibeGrantError::UnauthorizedReviewer
    )]
    pub reviewer: Signer<'info>,

    /// CHECK: Recipient receiving the escrowed funds
    #[account(mut, address = milestone_account.student)]
    pub student: AccountInfo<'info>,

    #[account(
        mut,
        has_one = reviewer,
        constraint = grant_account.is_active
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"milestone", grant_account.key().as_ref(), &[milestone_account.milestone_id]],
        bump = milestone_account.bump,
        has_one = grant
    )]
    pub milestone_account: Account<'info, MilestoneAccount>,

    /// CHECK: PDA Treasury vault containing the funds
    #[account(
        mut,
        seeds = [b"vault", grant_account.key().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<ApproveMilestone>) -> Result<()> {
    let milestone = &mut ctx.accounts.milestone_account;
    require!(milestone.status == MilestoneStatus::Pending, VibeGrantError::MilestoneAlreadyProcessed);

    let grant = &mut ctx.accounts.grant_account;
    require!(grant.remaining_budget >= milestone.amount, VibeGrantError::InsufficientGrantFunds);

    grant.remaining_budget = grant
        .remaining_budget
        .checked_sub(milestone.amount)
        .ok_or(VibeGrantError::NumericalOverflow)?;

    milestone.status = MilestoneStatus::Approved;

    // Disburse lamports directly from vault PDA to student
    let grant_key = grant.key();
    let vault_bump = grant.vault_bump;
    let signer_seeds: &[&[&[u8]]] = &[&[
        b"vault",
        grant_key.as_ref(),
        &[vault_bump],
    ]];

    anchor_lang::system_program::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            anchor_lang::system_program::Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.student.to_account_info(),
            },
            signer_seeds,
        ),
        milestone.amount,
    )?;

    msg!("Milestone {} approved. Lamports disbursed: {}", milestone.milestone_id, milestone.amount);
    Ok(())
}