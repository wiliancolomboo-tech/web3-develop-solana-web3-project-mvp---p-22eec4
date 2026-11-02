use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("CampVibe111111111111111111111111111111111111");

#[program]
pub mod campus_vibe {
    use super::*;

    /// Initialize a new milestone-locked grant with funds deposited into a program vault PDA.
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: String,
        title: String,
        total_milestones: u8,
        total_amount: u64,
    ) -> Result<()> {
        require!(grant_id.as_bytes().len() <= 32, CampusVibeError::IdentifierTooLong);
        require!(title.as_bytes().len() <= 64, CampusVibeError::TitleTooLong);
        require!(total_milestones > 0, CampusVibeError::ZeroMilestones);
        require!(total_amount > 0, CampusVibeError::ZeroFundingAmount);

        let grant = &mut ctx.accounts.grant;
        grant.sponsor = ctx.accounts.sponsor.key();
        grant.grantee = ctx.accounts.grantee.key();
        grant.reviewer = ctx.accounts.reviewer.key();
        grant.grant_id = grant_id.clone();
        grant.title = title;
        grant.total_milestones = total_milestones;
        grant.current_milestone = 0;
        grant.total_amount = total_amount;
        grant.amount_disbursed = 0;
        grant.milestone_amount = total_amount
            .checked_div(total_milestones as u64)
            .ok_or(CampusVibeError::ArithmeticOverflow)?;
        grant.is_active = true;
        grant.is_completed = false;
        grant.bump = ctx.bumps.grant;
        grant.vault_bump = ctx.bumps.vault;

        // Transfer funds from sponsor into the programmatic escrow vault
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sponsor.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        );
        transfer(cpi_context, total_amount)?;

        emit!(GrantCreatedEvent {
            grant_pubkey: grant.key(),
            sponsor: grant.sponsor,
            grantee: grant.grantee,
            grant_id,
            total_amount,
            total_milestones,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Grantee submits proof of work for the active milestone.
    pub fn submit_milestone(
        ctx: Context<SubmitMilestone>,
        milestone_index: u8,
        proof_uri: String,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(grant.is_active, CampusVibeError::GrantInactive);
        require!(!grant.is_completed, CampusVibeError::GrantAlreadyCompleted);
        require!(
            milestone_index == grant.current_milestone,
            CampusVibeError::InvalidMilestoneIndex
        );
        require!(proof_uri.as_bytes().len() <= 128, CampusVibeError::UriTooLong);

        let submission = &mut ctx.accounts.submission;
        submission.grant = grant.key();
        submission.milestone_index = milestone_index;
        submission.proof_uri = proof_uri.clone();
        submission.is_approved = false;
        submission.submitted_at = Clock::get()?.unix_timestamp;
        submission.bump = ctx.bumps.submission;

        emit!(MilestoneSubmittedEvent {
            grant_pubkey: grant.key(),
            grantee: grant.grantee,
            milestone_index,
            proof_uri,
            timestamp: submission.submitted_at,
        });

        Ok(())
    }

    /// Authorized reviewer approves the submitted milestone and triggers atomic release of tranche funds.
    pub fn evaluate_milestone(
        ctx: Context<EvaluateMilestone>,
        milestone_index: u8,
        approved: bool,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let submission = &mut ctx.accounts.submission;

        require!(grant.is_active, CampusVibeError::GrantInactive);
        require!(!grant.is_completed, CampusVibeError::GrantAlreadyCompleted);
        require!(
            milestone_index == grant.current_milestone,
            CampusVibeError::InvalidMilestoneIndex
        );
        require!(!submission.is_approved, CampusVibeError::MilestoneAlreadyApproved);

        let clock = Clock::get()?;

        if approved {
            submission.is_approved = true;

            // Calculate tranche payout
            let payout = if grant.current_milestone == grant.total_milestones - 1 {
                // Ensure dust is disbursed completely on the final milestone
                grant
                    .total_amount
                    .checked_sub(grant.amount_disbursed)
                    .ok_or(CampusVibeError::ArithmeticOverflow)?
            } else {
                grant.milestone_amount
            };

            grant.amount_disbursed = grant
                .amount_disbursed
                .checked_add(payout)
                .ok_or(CampusVibeError::ArithmeticOverflow)?;

            grant.current_milestone = grant
                .current_milestone
                .checked_add(1)
                .ok_or(CampusVibeError::ArithmeticOverflow)?;

            if grant.current_milestone >= grant.total_milestones {
                grant.is_completed = true;
                grant.is_active = false;
            }

            // Transfer tranche from Vault PDA to Grantee using PDA signer seeds
            let grant_key = grant.key();
            let vault_seeds: &[&[&[u8]]] = &[&[
                b"vault",
                grant_key.as_ref(),
                &[grant.vault_bump],
            ]];

            let cpi_context = CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to: ctx.accounts.grantee.to_account_info(),
                },
                vault_seeds,
            );
            transfer(cpi_context, payout)?;

            emit!(MilestoneEvaluatedEvent {
                grant_pubkey: grant.key(),
                milestone_index,
                approved: true,
                payout_amount: payout,
                recipient: grant.grantee,
                timestamp: clock.unix_timestamp,
            });
        } else {
            emit!(MilestoneEvaluatedEvent {
                grant_pubkey: grant.key(),
                milestone_index,
                approved: false,
                payout_amount: 0,
                recipient: grant.grantee,
                timestamp: clock.unix_timestamp,
            });
        }

        Ok(())
    }

    /// Sponsor cancellation mechanism for unallocated escrow funds in case of abandoned milestones.
    pub fn cancel_and_refund(ctx: Context<CancelAndRefund>) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(grant.is_active, CampusVibeError::GrantInactive);
        require!(!grant.is_completed, CampusVibeError::GrantAlreadyCompleted);

        let remaining_balance = ctx.accounts.vault.to_account_info().lamports();
        require!(remaining_balance > 0, CampusVibeError::ZeroBalanceInVault);

        let grant_key = grant.key();
        let vault_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            grant_key.as_ref(),
            &[grant.vault_bump],
        ]];

        let cpi_context = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.vault.to_account_info(),
                to: ctx.accounts.sponsor.to_account_info(),
            },
            vault_seeds,
        );
        transfer(cpi_context, remaining_balance)?;

        grant.is_active = false;

        emit!(GrantCancelledEvent {
            grant_pubkey: grant.key(),
            sponsor: grant.sponsor,
            refunded_amount: remaining_balance,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(grant_id: String)]
pub struct InitializeGrant<'info> {
    #[account(
        init,
        payer = sponsor,
        space = GrantAccount::LEN,
        seeds = [b"grant", sponsor.key().as_ref(), grant_id.as_bytes()],
        bump
    )]
    pub grant: Account<'info, GrantAccount>,

    /// CHECK: Program vault PDA holding escrowed SOL
    #[account(
        mut,
        seeds = [b"vault", grant.key().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    /// CHECK: Verified campus grantee
    pub grantee: AccountInfo<'info>,

    /// CHECK: Appointed review officer / collegiate DAO authority
    pub reviewer: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct SubmitMilestone<'info> {
    #[account(
        mut,
        has_one = grantee @ CampusVibeError::UnauthorizedGrantee,
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        init,
        payer = grantee,
        space = MilestoneSubmission::LEN,
        seeds = [b"submission", grant.key().as_ref(), &[milestone_index]],
        bump
    )]
    pub submission: Account<'info, MilestoneSubmission>,

    #[account(mut)]
    pub grantee: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct EvaluateMilestone<'info> {
    #[account(
        mut,
        has_one = reviewer @ CampusVibeError::UnauthorizedReviewer,
        has_one = grantee @ CampusVibeError::InvalidGranteeTarget,
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"submission", grant.key().as_ref(), &[milestone_index]],
        bump = submission.bump
    )]
    pub submission: Account<'info, MilestoneSubmission>,

    /// CHECK: Program Vault PDA holding escrowed funds
    #[account(
        mut,
        seeds = [b"vault", grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    pub reviewer: Signer<'info>,

    /// CHECK: Receives milestone payout
    #[account(mut)]
    pub grantee: AccountInfo<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CancelAndRefund<'info> {
    #[account(
        mut,
        has_one = sponsor @ CampusVibeError::UnauthorizedSponsor,
    )]
    pub grant: Account<'info, GrantAccount>,

    /// CHECK: Program vault PDA
    #[account(
        mut,
        seeds = [b"vault", grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// DATA STATE STRUCTS
// -----------------------------------------------------------------------------

#[account]
pub struct GrantAccount {
    pub sponsor: Pubkey,         // 32
    pub grantee: Pubkey,         // 32
    pub reviewer: Pubkey,        // 32
    pub grant_id: String,        // 4 + 32
    pub title: String,           // 4 + 64
    pub total_milestones: u8,    // 1
    pub current_milestone: u8,   // 1
    pub total_amount: u64,       // 8
    pub milestone_amount: u64,   // 8
    pub amount_disbursed: u64,   // 8
    pub is_active: bool,         // 1
    pub is_completed: bool,      // 1
    pub bump: u8,                // 1
    pub vault_bump: u8,          // 1
}

impl GrantAccount {
    pub const LEN: usize = 8 + 32 + 32 + 32 + (4 + 32) + (4 + 64) + 1 + 1 + 8 + 8 + 8 + 1 + 1 + 1 + 1 + 64;
}

#[account]
pub struct MilestoneSubmission {
    pub grant: Pubkey,           // 32
    pub milestone_index: u8,     // 1
    pub proof_uri: String,       // 4 + 128
    pub is_approved: bool,       // 1
    pub submitted_at: i64,       // 8
    pub bump: u8,                // 1
}

impl MilestoneSubmission {
    pub const LEN: usize = 8 + 32 + 1 + (4 + 128) + 1 + 8 + 1 + 32;
}

// -----------------------------------------------------------------------------
// EVENT DEFINITIONS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantCreatedEvent {
    pub grant_pubkey: Pubkey,
    pub sponsor: Pubkey,
    pub grantee: Pubkey,
    pub grant_id: String,
    pub total_amount: u64,
    pub total_milestones: u8,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub grant_pubkey: Pubkey,
    pub grantee: Pubkey,
    pub milestone_index: u8,
    pub proof_uri: String,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneEvaluatedEvent {
    pub grant_pubkey: Pubkey,
    pub milestone_index: u8,
    pub approved: bool,
    pub payout_amount: u64,
    pub recipient: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct GrantCancelledEvent {
    pub grant_pubkey: Pubkey,
    pub sponsor: Pubkey,
    pub refunded_amount: u64,
    pub timestamp: i64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum CampusVibeError {
    #[msg("Grant ID length cannot exceed 32 bytes.")]
    IdentifierTooLong,
    #[msg("Title length cannot exceed 64 bytes.")]
    TitleTooLong,
    #[msg("Grants must have at least one milestone.")]
    ZeroMilestones,
    #[msg("Grant funding amount must be strictly greater than zero.")]
    ZeroFundingAmount,
    #[msg("Math calculation resulted in an arithmetic overflow.")]
    ArithmeticOverflow,
    #[msg("The requested grant is no longer active.")]
    GrantInactive,
    #[msg("The grant has already completed all milestones.")]
    GrantAlreadyCompleted,
    #[msg("Invalid milestone index provided.")]
    InvalidMilestoneIndex,
    #[msg("Proof URI length cannot exceed 128 bytes.")]
    UriTooLong,
    #[msg("Signer is not the authorized grantee.")]
    UnauthorizedGrantee,
    #[msg("Signer is not the designated reviewer.")]
    UnauthorizedReviewer,
    #[msg("Target grantee mismatch with contract state.")]
    InvalidGranteeTarget,
    #[msg("Milestone has already been reviewed and approved.")]
    MilestoneAlreadyApproved,
    #[msg("Only the original sponsor can cancel an active grant.")]
    UnauthorizedSponsor,
    #[msg("Program vault has zero balance to refund.")]
    ZeroBalanceInVault,
}