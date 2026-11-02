use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("UniVibe111111111111111111111111111111111111");

#[program]
pub mod univibe_core {
    use super::*;

    /// Initializes a deterministic micro-grant vault
    pub fn initialize_grant_vault(
        ctx: Context<InitializeGrantVault>,
        grant_id: String,
        title: String,
        total_milestones: u8,
        total_target_lamports: u64,
    ) -> Result<()> {
        require!(grant_id.as_bytes().len() <= 32, UniVibeError::IdentifierTooLong);
        require!(title.as_bytes().len() <= 64, UniVibeError::TitleTooLong);
        require!(total_milestones > 0, UniVibeError::ZeroMilestonesInvalid);
        require!(total_target_lamports > 0, UniVibeError::ZeroAmountInvalid);

        let vault = &mut ctx.accounts.grant_vault;
        vault.sponsor = ctx.accounts.sponsor.key();
        vault.recipient = ctx.accounts.recipient.key();
        vault.evaluator = ctx.accounts.evaluator.key();
        vault.grant_id = grant_id;
        vault.title = title;
        vault.total_milestones = total_milestones;
        vault.current_milestone = 0;
        vault.total_target_lamports = total_target_lamports;
        vault.funded_lamports = 0;
        vault.disbursed_lamports = 0;
        vault.deliverable_hash = [0u8; 32];
        vault.state = GrantState::Created;
        vault.bump = ctx.bumps.grant_vault;
        vault.escrow_bump = ctx.bumps.escrow_wallet;

        emit!(GrantVaultInitializedEvent {
            grant_id: vault.grant_id.clone(),
            sponsor: vault.sponsor,
            recipient: vault.recipient,
            total_target_lamports,
            total_milestones,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Deposits lamports into the grant's dedicated PDA escrow account
    pub fn fund_grant(ctx: Context<FundGrant>, amount: u64) -> Result<()> {
        require!(amount > 0, UniVibeError::ZeroAmountInvalid);
        let vault = &mut ctx.accounts.grant_vault;

        require!(
            vault.state == GrantState::Created || vault.state == GrantState::Funded,
            UniVibeError::InvalidGrantState
        );

        // Perform CPI transfer of Native SOL from sponsor to Escrow PDA
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sponsor.to_account_info(),
                to: ctx.accounts.escrow_wallet.to_account_info(),
            },
        );
        transfer(cpi_context, amount)?;

        vault.funded_lamports = vault
            .funded_lamports
            .checked_add(amount)
            .ok_or(UniVibeError::MathOverflow)?;

        if vault.funded_lamports >= vault.total_target_lamports {
            vault.state = GrantState::Funded;
        }

        emit!(GrantFundedEvent {
            grant_id: vault.grant_id.clone(),
            sponsor: ctx.accounts.sponsor.key(),
            amount,
            total_funded: vault.funded_lamports,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Student submits SHA-256 deliverable commit proof for current milestone
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        deliverable_hash: [u8; 32],
    ) -> Result<()> {
        let vault = &mut ctx.accounts.grant_vault;

        require!(vault.state == GrantState::Funded, UniVibeError::NotFunded);
        require!(
            milestone_index == vault.current_milestone + 1,
            UniVibeError::MilestoneSequenceMismatch
        );
        require!(
            milestone_index <= vault.total_milestones,
            UniVibeError::MilestonesExceeded
        );

        vault.deliverable_hash = deliverable_hash;
        vault.state = GrantState::UnderReview;

        emit!(MilestoneSubmittedEvent {
            grant_id: vault.grant_id.clone(),
            milestone_index,
            deliverable_hash,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Evaluator attests deliverable completion and automatically disburses tranche
    pub fn approve_and_disburse(ctx: Context<ApproveAndDisburse>) -> Result<()> {
        let vault = &mut ctx.accounts.grant_vault;

        require!(
            vault.state == GrantState::UnderReview,
            UniVibeError::InvalidGrantState
        );

        // Calculate tranche amount
        let remaining_milestones = (vault.total_milestones - vault.current_milestone) as u64;
        let remaining_lamports = vault
            .total_target_lamports
            .checked_sub(vault.disbursed_lamports)
            .ok_or(UniVibeError::MathOverflow)?;

        let tranche_amount = remaining_lamports
            .checked_div(remaining_milestones)
            .ok_or(UniVibeError::MathOverflow)?;

        require!(
            ctx.accounts.escrow_wallet.lamports() >= tranche_amount,
            UniVibeError::InsufficientEscrowBalance
        );

        // PDA signer seeds for Lamport transfer
        let grant_id_bytes = vault.grant_id.as_bytes();
        let escrow_bump = vault.escrow_bump;
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"escrow",
            grant_id_bytes,
            &[escrow_bump],
        ]];

        let cpi_context = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.escrow_wallet.to_account_info(),
                to: ctx.accounts.recipient.to_account_info(),
            },
            signer_seeds,
        );
        transfer(cpi_context, tranche_amount)?;

        vault.current_milestone += 1;
        vault.disbursed_lamports = vault
            .disbursed_lamports
            .checked_add(tranche_amount)
            .ok_or(UniVibeError::MathOverflow)?;

        if vault.current_milestone == vault.total_milestones {
            vault.state = GrantState::Completed;
        } else {
            vault.state = GrantState::Funded; // Ready for next milestone
        }

        emit!(MilestoneApprovedEvent {
            grant_id: vault.grant_id.clone(),
            milestone_index: vault.current_milestone,
            disbursed_amount: tranche_amount,
            total_disbursed: vault.disbursed_lamports,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Sponsor refunds unearned lamports if student defaults or terms violated
    pub fn emergency_refund(ctx: Context<EmergencyRefund>) -> Result<()> {
        let vault = &mut ctx.accounts.grant_vault;

        require!(
            vault.state != GrantState::Completed,
            UniVibeError::AlreadyCompleted
        );

        let remaining_in_escrow = ctx.accounts.escrow_wallet.lamports();
        require!(remaining_in_escrow > 0, UniVibeError::ZeroBalanceInEscrow);

        let grant_id_bytes = vault.grant_id.as_bytes();
        let escrow_bump = vault.escrow_bump;
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"escrow",
            grant_id_bytes,
            &[escrow_bump],
        ]];

        let cpi_context = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.escrow_wallet.to_account_info(),
                to: ctx.accounts.sponsor.to_account_info(),
            },
            signer_seeds,
        );
        transfer(cpi_context, remaining_in_escrow)?;

        vault.state = GrantState::Cancelled;

        emit!(GrantCancelledRefundedEvent {
            grant_id: vault.grant_id.clone(),
            refunded_amount: remaining_in_escrow,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(grant_id: String)]
pub struct InitializeGrantVault<'info> {
    #[account(
        init,
        payer = sponsor,
        space = GrantVault::MAX_SIZE,
        seeds = [b"grant", grant_id.as_bytes()],
        bump
    )]
    pub grant_vault: Account<'info, GrantVault>,

    /// CHECK: Escrow account PDA solely holds lamports, governed by program seeds
    #[account(
        mut,
        seeds = [b"escrow", grant_id.as_bytes()],
        bump
    )]
    pub escrow_wallet: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    /// CHECK: Validated as recipient wallet
    pub recipient: AccountInfo<'info>,

    /// CHECK: Validated as evaluator authority
    pub evaluator: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FundGrant<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.bump,
        has_one = sponsor @ UniVibeError::UnauthorizedSponsor
    )]
    pub grant_vault: Account<'info, GrantVault>,

    /// CHECK: Verified against grant PDA
    #[account(
        mut,
        seeds = [b"escrow", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.escrow_bump
    )]
    pub escrow_wallet: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitMilestone<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.bump,
        has_one = recipient @ UniVibeError::UnauthorizedRecipient
    )]
    pub grant_vault: Account<'info, GrantVault>,

    pub recipient: Signer<'info>,
}

#[derive(Accounts)]
pub struct ApproveAndDisburse<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.bump,
        has_one = evaluator @ UniVibeError::UnauthorizedEvaluator,
        has_one = recipient @ UniVibeError::RecipientMismatch
    )]
    pub grant_vault: Account<'info, GrantVault>,

    /// CHECK: Checked by PDA seeds
    #[account(
        mut,
        seeds = [b"escrow", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.escrow_bump
    )]
    pub escrow_wallet: SystemAccount<'info>,

    pub evaluator: Signer<'info>,

    /// CHECK: Direct recipient account receiving Lamports
    #[account(mut)]
    pub recipient: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EmergencyRefund<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.bump,
        has_one = sponsor @ UniVibeError::UnauthorizedSponsor
    )]
    pub grant_vault: Account<'info, GrantVault>,

    /// CHECK: Verified by PDA seeds
    #[account(
        mut,
        seeds = [b"escrow", grant_vault.grant_id.as_bytes()],
        bump = grant_vault.escrow_bump
    )]
    pub escrow_wallet: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// STATE DEFINITIONS
// -----------------------------------------------------------------------------

#[account]
pub struct GrantVault {
    pub sponsor: Pubkey,          // 32
    pub recipient: Pubkey,        // 32
    pub evaluator: Pubkey,        // 32
    pub grant_id: String,         // 4 + 32
    pub title: String,            // 4 + 64
    pub total_milestones: u8,     // 1
    pub current_milestone: u8,    // 1
    pub total_target_lamports: u64, // 8
    pub funded_lamports: u64,     // 8
    pub disbursed_lamports: u64,  // 8
    pub deliverable_hash: [u8; 32], // 32
    pub state: GrantState,        // 1
    pub bump: u8,                 // 1
    pub escrow_bump: u8,          // 1
}

impl GrantVault {
    pub const MAX_SIZE: usize = 8 + 32 + 32 + 32 + (4 + 32) + (4 + 64) + 1 + 1 + 8 + 8 + 8 + 32 + 1 + 1 + 1 + 64;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum GrantState {
    Created,
    Funded,
    UnderReview,
    Completed,
    Cancelled,
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantVaultInitializedEvent {
    pub grant_id: String,
    pub sponsor: Pubkey,
    pub recipient: Pubkey,
    pub total_target_lamports: u64,
    pub total_milestones: u8,
    pub timestamp: i64,
}

#[event]
pub struct GrantFundedEvent {
    pub grant_id: String,
    pub sponsor: Pubkey,
    pub amount: u64,
    pub total_funded: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub grant_id: String,
    pub milestone_index: u8,
    pub deliverable_hash: [u8; 32],
    pub timestamp: i64,
}

#[event]
pub struct MilestoneApprovedEvent {
    pub grant_id: String,
    pub milestone_index: u8,
    pub disbursed_amount: u64,
    pub total_disbursed: u64,
    pub timestamp: i64,
}

#[event]
pub struct GrantCancelledRefundedEvent {
    pub grant_id: String,
    pub refunded_amount: u64,
    pub timestamp: i64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum UniVibeError {
    #[msg("Grant ID string must be 32 bytes or fewer.")]
    IdentifierTooLong,
    #[msg("Title must be 64 bytes or fewer.")]
    TitleTooLong,
    #[msg("Milestone count must be greater than zero.")]
    ZeroMilestonesInvalid,
    #[msg("Amount must be greater than zero.")]
    ZeroAmountInvalid,
    #[msg("Grant is not in a valid state for this operation.")]
    InvalidGrantState,
    #[msg("Grant has not been fully funded yet.")]
    NotFunded,
    #[msg("Submitted milestone index is out of linear order.")]
    MilestoneSequenceMismatch,
    #[msg("All milestones have already been submitted.")]
    MilestonesExceeded,
    #[msg("Escrow account lacks sufficient Lamports for disbursement.")]
    InsufficientEscrowBalance,
    #[msg("Signer is not authorized sponsor.")]
    UnauthorizedSponsor,
    #[msg("Signer is not authorized recipient.")]
    UnauthorizedRecipient,
    #[msg("Signer is not authorized evaluator.")]
    UnauthorizedEvaluator,
    #[msg("Supplied recipient account differs from vault recipient.")]
    RecipientMismatch,
    #[msg("Grant is already completed.")]
    AlreadyCompleted,
    #[msg("Escrow account contains zero Lamports.")]
    ZeroBalanceInEscrow,
    #[msg("Math calculation resulted in integer overflow or division by zero.")]
    MathOverflow,
}