use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("V1beZJ2G245F23i9k78pP75wMh2XQ87y3NqU7kK9m3N");

pub const MAX_TRANCHES: usize = 5;
pub const MAX_NAME_LEN: usize = 48;
pub const MAX_URI_LEN: usize = 128;
pub const ESCROW_SEED: &[u8] = b"univibe_escrow";
pub const VAULT_SEED: &[u8] = b"univibe_vault";
pub const MILESTONE_SEED: &[u8] = b"univibe_milestone";

#[program]
pub mod univibe {
    use super::*;

    /// Initializes a newly sponsored grant escrow with defined milestone tranches.
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        grant_id: u64,
        project_name: String,
        tranche_amounts: Vec<u64>,
        deadline_timestamp: i64,
    ) -> Result<()> {
        require!(
            !project_name.is_empty() && project_name.len() <= MAX_NAME_LEN,
            UniVibeError::InvalidProjectName
        );
        require!(
            !tranche_amounts.is_empty() && tranche_amounts.len() <= MAX_TRANCHES,
            UniVibeError::InvalidTrancheCount
        );

        let clock = Clock::get()?;
        require!(
            deadline_timestamp > clock.unix_timestamp,
            UniVibeError::InvalidDeadline
        );

        let mut total_grant_amount: u64 = 0;
        for &amt in tranche_amounts.iter() {
            require!(amt > 0, UniVibeError::ZeroTrancheAmount);
            total_grant_amount = total_grant_amount
                .checked_add(amt)
                .ok_or(UniVibeError::MathOverflow)?;
        }

        // Fund the vault escrow account from the sponsor
        let cpi_ctx = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sponsor.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        );
        transfer(cpi_ctx, total_grant_amount)?;

        let escrow = &mut ctx.accounts.grant_escrow;
        escrow.grant_id = grant_id;
        escrow.sponsor = ctx.accounts.sponsor.key();
        escrow.grantee = ctx.accounts.grantee.key();
        escrow.evaluator = ctx.accounts.evaluator.key();
        escrow.total_amount = total_grant_amount;
        escrow.released_amount = 0;
        escrow.tranche_count = tranche_amounts.len() as u8;
        escrow.completed_tranches = 0;
        escrow.deadline_timestamp = deadline_timestamp;
        escrow.is_cancelled = false;
        escrow.escrow_bump = ctx.bumps.grant_escrow;
        escrow.vault_bump = ctx.bumps.vault;
        escrow.project_name = project_name.clone();

        for (idx, &amt) in tranche_amounts.iter().enumerate() {
            escrow.tranche_allocations[idx] = amt;
        }

        emit!(GrantCreatedEvent {
            grant_id,
            sponsor: ctx.accounts.sponsor.key(),
            grantee: ctx.accounts.grantee.key(),
            total_amount: total_grant_amount,
            tranches: escrow.tranche_count,
            project_name,
        });

        Ok(())
    }

    /// Submits milestone proof (commit hash / build hash) by the grantee team.
    pub fn submit_milestone_proof(
        ctx: Context<SubmitMilestoneProof>,
        milestone_index: u8,
        proof_hash: [u8; 32],
        metadata_uri: String,
    ) -> Result<()> {
        let escrow = &ctx.accounts.grant_escrow;
        require!(!escrow.is_cancelled, UniVibeError::GrantAlreadyCancelled);
        require!(
            milestone_index == escrow.completed_tranches,
            UniVibeError::InvalidMilestoneSequence
        );
        require!(
            (milestone_index as usize) < (escrow.tranche_count as usize),
            UniVibeError::IndexOutOfBounds
        );
        require!(
            metadata_uri.len() <= MAX_URI_LEN,
            UniVibeError::UriTooLong
        );

        let milestone = &mut ctx.accounts.milestone_record;
        milestone.grant_id = escrow.grant_id;
        milestone.milestone_index = milestone_index;
        milestone.proof_hash = proof_hash;
        milestone.metadata_uri = metadata_uri.clone();
        milestone.submitted_at = Clock::get()?.unix_timestamp;
        milestone.is_approved = false;
        milestone.bump = ctx.bumps.milestone_record;

        emit!(MilestoneSubmittedEvent {
            grant_id: escrow.grant_id,
            milestone_index,
            proof_hash,
            metadata_uri,
        });

        Ok(())
    }

    /// Evaluator attests to milestone proof, immediately triggering tranche payment.
    pub fn attest_and_release(
        ctx: Context<AttestAndRelease>,
        milestone_index: u8,
    ) -> Result<()> {
        let escrow = &mut ctx.accounts.grant_escrow;
        let milestone = &mut ctx.accounts.milestone_record;

        require!(!escrow.is_cancelled, UniVibeError::GrantAlreadyCancelled);
        require!(
            milestone_index == escrow.completed_tranches,
            UniVibeError::InvalidMilestoneSequence
        );
        require!(!milestone.is_approved, UniVibeError::MilestoneAlreadyApproved);

        let payout_amount = escrow.tranche_allocations[milestone_index as usize];
        require!(payout_amount > 0, UniVibeError::InvalidPayoutAmount);

        // State update prior to execution
        milestone.is_approved = true;
        escrow.completed_tranches = escrow
            .completed_tranches
            .checked_add(1)
            .ok_or(UniVibeError::MathOverflow)?;
        escrow.released_amount = escrow
            .released_amount
            .checked_add(payout_amount)
            .ok_or(UniVibeError::MathOverflow)?;

        // Execute CPI payout from PDA Vault to Grantee
        let grant_id_bytes = escrow.grant_id.to_le_bytes();
        let seeds: &[&[&[u8]]] = &[&[
            VAULT_SEED,
            grant_id_bytes.as_ref(),
            &[escrow.vault_bump],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.grantee.to_account_info(),
        };
        let cpi_program = ctx.accounts.system_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, seeds);

        transfer(cpi_ctx, payout_amount)?;

        emit!(MilestoneApprovedEvent {
            grant_id: escrow.grant_id,
            milestone_index,
            payout_amount,
            grantee: ctx.accounts.grantee.key(),
            total_released: escrow.released_amount,
        });

        Ok(())
    }

    /// Sponsor emergency clawback if deadline passed and milestones remain unfulfilled.
    pub fn emergency_refund(ctx: Context<EmergencyRefund>) -> Result<()> {
        let escrow = &mut ctx.accounts.grant_escrow;
        let clock = Clock::get()?;

        require!(!escrow.is_cancelled, UniVibeError::GrantAlreadyCancelled);
        require!(
            clock.unix_timestamp > escrow.deadline_timestamp,
            UniVibeError::DeadlineNotReached
        );
        require!(
            escrow.completed_tranches < escrow.tranche_count,
            UniVibeError::AllTranchesCompleted
        );

        let remaining_balance = ctx.accounts.vault.lamports();
        require!(remaining_balance > 0, UniVibeError::VaultEmpty);

        escrow.is_cancelled = true;

        let grant_id_bytes = escrow.grant_id.to_le_bytes();
        let seeds: &[&[&[u8]]] = &[&[
            VAULT_SEED,
            grant_id_bytes.as_ref(),
            &[escrow.vault_bump],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault.to_account_info(),
            to: ctx.accounts.sponsor.to_account_info(),
        };
        let cpi_program = ctx.accounts.system_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, seeds);

        transfer(cpi_ctx, remaining_balance)?;

        emit!(GrantRefundedEvent {
            grant_id: escrow.grant_id,
            sponsor: ctx.accounts.sponsor.key(),
            refunded_amount: remaining_balance,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(grant_id: u64)]
pub struct InitializeGrant<'info> {
    #[account(mut)]
    pub sponsor: Signer<'info>,

    /// CHECK: Grantee student wallet receiving milestone unlocks
    pub grantee: AccountInfo<'info>,

    /// CHECK: University evaluator or trusted technical mentor
    pub evaluator: AccountInfo<'info>,

    #[account(
        init,
        payer = sponsor,
        space = GrantEscrow::LEN,
        seeds = [ESCROW_SEED, grant_id.to_le_bytes().as_ref()],
        bump
    )]
    pub grant_escrow: Account<'info, GrantEscrow>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant_id.to_le_bytes().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct SubmitMilestoneProof<'info> {
    #[account(mut)]
    pub grantee: Signer<'info>,

    #[account(
        seeds = [ESCROW_SEED, grant_escrow.grant_id.to_le_bytes().as_ref()],
        bump = grant_escrow.escrow_bump,
        has_one = grantee @ UniVibeError::UnauthorizedGrantee
    )]
    pub grant_escrow: Account<'info, GrantEscrow>,

    #[account(
        init,
        payer = grantee,
        space = MilestoneRecord::LEN,
        seeds = [
            MILESTONE_SEED,
            grant_escrow.grant_id.to_le_bytes().as_ref(),
            &[milestone_index]
        ],
        bump
    )]
    pub milestone_record: Account<'info, MilestoneRecord>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct AttestAndRelease<'info> {
    #[account(mut)]
    pub evaluator: Signer<'info>,

    /// CHECK: Validated against grant_escrow.grantee
    #[account(
        mut,
        address = grant_escrow.grantee @ UniVibeError::UnauthorizedGrantee
    )]
    pub grantee: AccountInfo<'info>,

    #[account(
        mut,
        seeds = [ESCROW_SEED, grant_escrow.grant_id.to_le_bytes().as_ref()],
        bump = grant_escrow.escrow_bump,
        has_one = evaluator @ UniVibeError::UnauthorizedEvaluator
    )]
    pub grant_escrow: Account<'info, GrantEscrow>,

    #[account(
        mut,
        seeds = [
            MILESTONE_SEED,
            grant_escrow.grant_id.to_le_bytes().as_ref(),
            &[milestone_index]
        ],
        bump = milestone_record.bump
    )]
    pub milestone_record: Account<'info, MilestoneRecord>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant_escrow.grant_id.to_le_bytes().as_ref()],
        bump = grant_escrow.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EmergencyRefund<'info> {
    #[account(
        mut,
        address = grant_escrow.sponsor @ UniVibeError::UnauthorizedSponsor
    )]
    pub sponsor: Signer<'info>,

    #[account(
        mut,
        seeds = [ESCROW_SEED, grant_escrow.grant_id.to_le_bytes().as_ref()],
        bump = grant_escrow.escrow_bump,
        has_one = sponsor @ UniVibeError::UnauthorizedSponsor
    )]
    pub grant_escrow: Account<'info, GrantEscrow>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant_escrow.grant_id.to_le_bytes().as_ref()],
        bump = grant_escrow.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// STORAGE LAYOUTS
// -----------------------------------------------------------------------------

#[account]
pub struct GrantEscrow {
    pub grant_id: u64,                        // 8
    pub sponsor: Pubkey,                      // 32
    pub grantee: Pubkey,                      // 32
    pub evaluator: Pubkey,                    // 32
    pub total_amount: u64,                    // 8
    pub released_amount: u64,                 // 8
    pub tranche_count: u8,                    // 1
    pub completed_tranches: u8,               // 1
    pub deadline_timestamp: i64,              // 8
    pub is_cancelled: bool,                   // 1
    pub escrow_bump: u8,                      // 1
    pub vault_bump: u8,                       // 1
    pub project_name: String,                 // 4 + 48
    pub tranche_allocations: [u64; MAX_TRANCHES], // 8 * 5 = 40
}

impl GrantEscrow {
    pub const LEN: usize = 8 + 8 + 32 + 32 + 32 + 8 + 8 + 1 + 1 + 8 + 1 + 1 + 1 + (4 + MAX_NAME_LEN) + (8 * MAX_TRANCHES) + 64;
}

#[account]
pub struct MilestoneRecord {
    pub grant_id: u64,         // 8
    pub milestone_index: u8,   // 1
    pub proof_hash: [u8; 32],  // 32
    pub metadata_uri: String,  // 4 + 128
    pub submitted_at: i64,     // 8
    pub is_approved: bool,     // 1
    pub bump: u8,              // 1
}

impl MilestoneRecord {
    pub const LEN: usize = 8 + 8 + 1 + 32 + (4 + MAX_URI_LEN) + 8 + 1 + 1 + 32;
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantCreatedEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub grantee: Pubkey,
    pub total_amount: u64,
    pub tranches: u8,
    pub project_name: String,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub proof_hash: [u8; 32],
    pub metadata_uri: String,
}

#[event]
pub struct MilestoneApprovedEvent {
    pub grant_id: u64,
    pub milestone_index: u8,
    pub payout_amount: u64,
    pub grantee: Pubkey,
    pub total_released: u64,
}

#[event]
pub struct GrantRefundedEvent {
    pub grant_id: u64,
    pub sponsor: Pubkey,
    pub refunded_amount: u64,
}

// -----------------------------------------------------------------------------
// ERROR CODES
// -----------------------------------------------------------------------------

#[error_code]
pub enum UniVibeError {
    #[msg("Project name must be between 1 and 48 characters.")]
    InvalidProjectName,
    #[msg("Tranche count must be between 1 and 5.")]
    InvalidTrancheCount,
    #[msg("Grant deadline must be set in the future.")]
    InvalidDeadline,
    #[msg("Tranche allocation amount cannot be zero.")]
    ZeroTrancheAmount,
    #[msg("Milestones must be submitted sequentially.")]
    InvalidMilestoneSequence,
    #[msg("Milestone index out of defined range.")]
    IndexOutOfBounds,
    #[msg("Metadata URI exceeds maximum allowed length of 128 bytes.")]
    UriTooLong,
    #[msg("This milestone has already received attestation and release.")]
    MilestoneAlreadyApproved,
    #[msg("Grant is cancelled or already finalized.")]
    GrantAlreadyCancelled,
    #[msg("Grant refund deadline has not yet been reached.")]
    DeadlineNotReached,
    #[msg("All tranches were fulfilled; no refund available.")]
    AllTranchesCompleted,
    #[msg("Escrow vault has zero balance.")]
    VaultEmpty,
    #[msg("Invalid payout calculation.")]
    InvalidPayoutAmount,
    #[msg("Signer is not authorized grantee.")]
    UnauthorizedGrantee,
    #[msg("Signer is not authorized evaluator.")]
    UnauthorizedEvaluator,
    #[msg("Signer is not authorized sponsor.")]
    UnauthorizedSponsor,
    #[msg("Arithmetic operation resulted in an overflow.")]
    MathOverflow,
}