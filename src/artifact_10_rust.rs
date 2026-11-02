use anchor_lang::prelude::*;
use crate::state::GrantAccount;
use crate::errors::VibeGrantError;

#[derive(Accounts)]
pub struct ClawbackExpired<'info> {
    #[account(
        mut,
        address = grant_account.sponsor
    )]
    pub sponsor: Signer<'info>,

    #[account(
        mut,
        has_one = sponsor,
        constraint = grant_account.is_active
    )]
    pub grant_account: Account<'info, GrantAccount>,

    /// CHECK: PDA Treasury vault holding remaining escrow lamports
    #[account(
        mut,
        seeds = [b"vault", grant_account.key().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(ctx: Context<ClawbackExpired>) -> Result<()> {
    let grant = &mut ctx.accounts.grant_account;
    let current_time = Clock::get()?.unix_timestamp;

    require!(current_time >= grant.deadline_epoch, VibeGrantError::GrantNotExpired);
    let unspent_amount = grant.remaining_budget;

    grant.remaining_budget = 0;
    grant.is_active = false;

    if unspent_amount > 0 {
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
                    to: ctx.accounts.sponsor.to_account_info(),
                },
                signer_seeds,
            ),
            unspent_amount,
        )?;
    }

    msg!("Grant expired. Clawback executed to sponsor for {} lamports.", unspent_amount);
    Ok(())
}