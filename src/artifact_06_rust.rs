use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("ViBE111111111111111111111111111111111111111");

#[program]
pub mod campus_vibe {
    use super::*;

    /// Initialize a new Campus Vibe Hub with sponsor authorization and metadata.
    pub fn initialize_hub(
        ctx: Context<InitializeHub>,
        hub_id: String,
        name: String,
        max_milestones: u16,
    ) -> Result<()> {
        require!(hub_id.len() > 0 && hub_id.len() <= 32, VibeError::InvalidStringLength);
        require!(name.len() > 0 && name.len() <= 64, VibeError::InvalidStringLength);
        require!(max_milestones > 0 && max_milestones <= 1000, VibeError::InvalidMilestoneLimit);

        let hub = &mut ctx.accounts.hub;
        hub.authority = ctx.accounts.authority.key();
        hub.hub_id = hub_id;
        hub.name = name;
        hub.total_deposited = 0;
        hub.total_distributed = 0;
        hub.milestone_count = 0;
        hub.max_milestones = max_milestones;
        hub.is_active = true;
        hub.bump = ctx.bumps.hub;

        emit!(HubCreatedEvent {
            hub: hub.key(),
            authority: hub.authority,
            hub_id: hub.hub_id.clone(),
            name: hub.name.clone(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Deposit funds into a designated Milestone Escrow under the Hub.
    pub fn create_milestone_escrow(
        ctx: Context<CreateMilestoneEscrow>,
        milestone_id: String,
        description: String,
        amount: u64,
    ) -> Result<()> {
        require!(amount > 0, VibeError::ZeroAmount);
        require!(milestone_id.len() > 0 && milestone_id.len() <= 32, VibeError::InvalidStringLength);
        require!(description.len() > 0 && description.len() <= 128, VibeError::InvalidStringLength);

        let hub = &mut ctx.accounts.hub;
        require!(hub.is_active, VibeError::HubInactive);
        require!(hub.milestone_count < hub.max_milestones, VibeError::MaxMilestonesReached);

        // Fund the escrow vault PDA via CPI to System Program
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sponsor.to_account_info(),
                to: ctx.accounts.escrow_vault.to_account_info(),
            },
        );
        transfer(cpi_context, amount)?;

        let milestone = &mut ctx.accounts.milestone;
        milestone.hub = hub.key();
        milestone.milestone_id = milestone_id.clone();
        milestone.description = description;
        milestone.amount = amount;
        milestone.beneficiary = Pubkey::default();
        milestone.is_completed = false;
        milestone.bump = ctx.bumps.milestone;
        milestone.vault_bump = ctx.bumps.escrow_vault;

        hub.total_deposited = hub.total_deposited.checked_add(amount).ok_or(VibeError::MathOverflow)?;
        hub.milestone_count = hub.milestone_count.checked_add(1).ok_or(VibeError::MathOverflow)?;

        emit!(MilestoneCreatedEvent {
            hub: hub.key(),
            milestone: milestone.key(),
            milestone_id,
            amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Approve milestone completion and release locked escrow funds directly to the student beneficiary.
    pub fn release_milestone(
        ctx: Context<ReleaseMilestone>,
        milestone_id: String,
    ) -> Result<()> {
        let hub = &mut ctx.accounts.hub;
        require!(hub.is_active, VibeError::HubInactive);

        let milestone = &mut ctx.accounts.milestone;
        require!(!milestone.is_completed, VibeError::MilestoneAlreadyCompleted);
        require!(milestone.milestone_id == milestone_id, VibeError::MilestoneMismatch);

        let amount = milestone.amount;
        let hub_key = hub.key();
        let milestone_seed = milestone.milestone_id.as_bytes();

        let vault_seeds = &[
            b"escrow_vault",
            hub_key.as_ref(),
            milestone_seed,
            &[milestone.vault_bump],
        ];
        let signer_seeds = &[&vault_seeds[..]];

        // Transfer funds from Escrow Vault PDA to Beneficiary
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.escrow_vault.to_account_info(),
                to: ctx.accounts.beneficiary.to_account_info(),
            },
            signer_seeds,
        );
        transfer(cpi_ctx, amount)?;

        milestone.beneficiary = ctx.accounts.beneficiary.key();
        milestone.is_completed = true;

        hub.total_distributed = hub.total_distributed.checked_add(amount).ok_or(VibeError::MathOverflow)?;

        emit!(MilestoneReleasedEvent {
            hub: hub.key(),
            milestone: milestone.key(),
            milestone_id: milestone.milestone_id.clone(),
            beneficiary: ctx.accounts.beneficiary.key(),
            amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Emergency cancellation of an incomplete milestone escrow by the authorized Hub authority.
    pub fn cancel_milestone(
        ctx: Context<CancelMilestone>,
        milestone_id: String,
    ) -> Result<()> {
        let hub = &mut ctx.accounts.hub;
        let milestone = &mut ctx.accounts.milestone;

        require!(!milestone.is_completed, VibeError::MilestoneAlreadyCompleted);
        require!(milestone.milestone_id == milestone_id, VibeError::MilestoneMismatch);

        let amount = ctx.accounts.escrow_vault.to_account_info().lamports();
        let hub_key = hub.key();
        let milestone_seed = milestone.milestone_id.as_bytes();

        let vault_seeds = &[
            b"escrow_vault",
            hub_key.as_ref(),
            milestone_seed,
            &[milestone.vault_bump],
        ];
        let signer_seeds = &[&vault_seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.escrow_vault.to_account_info(),
                to: ctx.accounts.authority.to_account_info(),
            },
            signer_seeds,
        );
        transfer(cpi_ctx, amount)?;

        milestone.is_completed = true; // Mark as closed
        hub.total_deposited = hub.total_deposited.saturating_sub(amount);

        emit!(MilestoneCancelledEvent {
            hub: hub.key(),
            milestone: milestone.key(),
            milestone_id: milestone.milestone_id.clone(),
            refund_amount: amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Account Contexts & State Schemas
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(hub_id: String)]
pub struct InitializeHub<'info> {
    #[account(
        init,
        payer = authority,
        space = VibeHub::LEN,
        seeds = [b"hub", authority.key().as_ref(), hub_id.as_bytes()],
        bump
    )]
    pub hub: Account<'info, VibeHub>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_id: String)]
pub struct CreateMilestoneEscrow<'info> {
    #[account(
        mut,
        has_one = authority @ VibeError::UnauthorizedAuthority
    )]
    pub hub: Account<'info, VibeHub>,

    #[account(
        init,
        payer = sponsor,
        space = MilestoneEscrow::LEN,
        seeds = [b"milestone", hub.key().as_ref(), milestone_id.as_bytes()],
        bump
    )]
    pub milestone: Account<'info, MilestoneEscrow>,

    /// CHECK: PDA used strictly as a SOL vault for locked escrow
    #[account(
        mut,
        seeds = [b"escrow_vault", hub.key().as_ref(), milestone_id.as_bytes()],
        bump
    )]
    pub escrow_vault: AccountInfo<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_id: String)]
pub struct ReleaseMilestone<'info> {
    #[account(
        mut,
        has_one = authority @ VibeError::UnauthorizedAuthority
    )]
    pub hub: Account<'info, VibeHub>,

    #[account(
        mut,
        seeds = [b"milestone", hub.key().as_ref(), milestone_id.as_bytes()],
        bump = milestone.bump,
        has_one = hub @ VibeError::InvalidHubAssociation
    )]
    pub milestone: Account<'info, MilestoneEscrow>,

    /// CHECK: Escrow vault PDA storing funds
    #[account(
        mut,
        seeds = [b"escrow_vault", hub.key().as_ref(), milestone_id.as_bytes()],
        bump = milestone.vault_bump
    )]
    pub escrow_vault: AccountInfo<'info>,

    /// CHECK: Beneficiary receiving the milestone payout
    #[account(mut)]
    pub beneficiary: AccountInfo<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_id: String)]
pub struct CancelMilestone<'info> {
    #[account(
        mut,
        has_one = authority @ VibeError::UnauthorizedAuthority
    )]
    pub hub: Account<'info, VibeHub>,

    #[account(
        mut,
        seeds = [b"milestone", hub.key().as_ref(), milestone_id.as_bytes()],
        bump = milestone.bump,
        has_one = hub @ VibeError::InvalidHubAssociation
    )]
    pub milestone: Account<'info, MilestoneEscrow>,

    /// CHECK: Escrow vault PDA
    #[account(
        mut,
        seeds = [b"escrow_vault", hub.key().as_ref(), milestone_id.as_bytes()],
        bump = milestone.vault_bump
    )]
    pub escrow_vault: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// Data Accounts
// -----------------------------------------------------------------------------

#[account]
pub struct VibeHub {
    pub authority: Pubkey,       // 32
    pub hub_id: String,          // 4 + 32
    pub name: String,            // 4 + 64
    pub total_deposited: u64,    // 8
    pub total_distributed: u64,  // 8
    pub milestone_count: u16,    // 2
    pub max_milestones: u16,     // 2
    pub is_active: bool,         // 1
    pub bump: u8,                // 1
}

impl VibeHub {
    pub const LEN: usize = 8 + 32 + (4 + 32) + (4 + 64) + 8 + 8 + 2 + 2 + 1 + 1 + 32;
}

#[account]
pub struct MilestoneEscrow {
    pub hub: Pubkey,             // 32
    pub milestone_id: String,    // 4 + 32
    pub description: String,     // 4 + 128
    pub amount: u64,             // 8
    pub beneficiary: Pubkey,     // 32
    pub is_completed: bool,      // 1
    pub bump: u8,                // 1
    pub vault_bump: u8,          // 1
}

impl MilestoneEscrow {
    pub const LEN: usize = 8 + 32 + (4 + 32) + (4 + 128) + 8 + 32 + 1 + 1 + 1 + 32;
}

// -----------------------------------------------------------------------------
// Events
// -----------------------------------------------------------------------------

#[event]
pub struct HubCreatedEvent {
    pub hub: Pubkey,
    pub authority: Pubkey,
    pub hub_id: String,
    pub name: String,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneCreatedEvent {
    pub hub: Pubkey,
    pub milestone: Pubkey,
    pub milestone_id: String,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneReleasedEvent {
    pub hub: Pubkey,
    pub milestone: Pubkey,
    pub milestone_id: String,
    pub beneficiary: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneCancelledEvent {
    pub hub: Pubkey,
    pub milestone: Pubkey,
    pub milestone_id: String,
    pub refund_amount: u64,
    pub timestamp: i64,
}

// -----------------------------------------------------------------------------
// Error Codes
// -----------------------------------------------------------------------------

#[error_code]
pub enum VibeError {
    #[msg("String exceeds maximum allowed length.")]
    InvalidStringLength,
    #[msg("Milestone limit must be between 1 and 1000.")]
    InvalidMilestoneLimit,
    #[msg("Deposit amount must be strictly greater than 0.")]
    ZeroAmount,
    #[msg("The campus vibe hub is currently inactive.")]
    HubInactive,
    #[msg("Maximum milestone capacity reached for this hub.")]
    MaxMilestonesReached,
    #[msg("Milestone has already been completed or cancelled.")]
    MilestoneAlreadyCompleted,
    #[msg("Provided milestone ID does not match the record.")]
    MilestoneMismatch,
    #[msg("Unauthorized: Signer is not the hub authority.")]
    UnauthorizedAuthority,
    #[msg("Escrow account is not associated with this hub.")]
    InvalidHubAssociation,
    #[msg("Mathematical overflow occurred during accounting.")]
    MathOverflow,
}