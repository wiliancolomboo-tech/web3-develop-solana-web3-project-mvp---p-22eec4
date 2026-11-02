use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};
use crate::state::GrantAccount;
use crate::errors::VibeGrantError;

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct InitializeGrant<'info> {
    #[account(mut)]
    pub sponsor: Signer<'info>,

    /// CHECK: Validated as designated non-signer reviewer authorized for grading
    pub reviewer: AccountInfo<'info>,

    #[account(
        init,
        payer = sponsor,
        space = GrantAccount::LEN,
        seeds = [b"grant", sponsor.key().as_ref(), &grant_id.to_le_bytes()],
        bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    /// CHECK: PDA Treasury vault solely holding escrowed SOL
    #[account(
        mut,
        seeds = [b"vault", grant_account.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(
    ctx: Context<InitializeGrant>,
    grant_id: u64,
    title: String,
    total_budget: u64,
    deadline_epoch: i64,
) -> Result<()> {
    require!(title.len() <= GrantAccount::MAXIMUM_TITLE_LENGTH, VibeGrantError::TitleTooLong);
    require!(total_budget > 0, VibeGrantError::ZeroBudget);
    let current_time = Clock::get()?.unix_timestamp;
    require!(deadline_epoch > current_time, VibeGrantError::InvalidDeadline);

    let grant = &mut ctx.accounts.grant_account;
    grant.sponsor = ctx.accounts.sponsor.key();
    grant.reviewer = ctx.accounts.reviewer.key();
    grant.grant_id = grant_id;
    grant.total_budget = total_budget;
    grant.remaining_budget = total_budget;
    grant.deadline_epoch = deadline_epoch;
    grant.is_active = true;
    grant.grant_bump = ctx.bumps.grant_account;
    grant.vault_bump = ctx.bumps.vault;
    grant.title = title;

    // Escrow transfer from sponsor to PDA vault
    let cpi_context = CpiContext::new(
        ctx.accounts.system_program.to_account_info(),
        Transfer {
            from: ctx.accounts.sponsor.to_account_info(),
            to: ctx.accounts.vault.to_account_info(),
        },
    );
    transfer(cpi_context, total_budget)?;

    msg!("Grant {} initialized. Total budget {} deposited to vault.", grant_id, total_budget);
    Ok(())
}