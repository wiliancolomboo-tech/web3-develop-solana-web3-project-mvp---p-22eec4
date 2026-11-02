use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("UniVibe111111111111111111111111111111111111");

#[program]
pub mod univibe {
    use super::*;

    /// Initializes a new Project Micro-Endowment campaign with multi-milestone escrow.
    pub fn initialize_project(
        ctx: Context<InitializeProject>,
        project_id: u64,
        title: String,
        metadata_uri: String,
        milestone_amounts: Vec<u64>,
        required_attestations: u8,
    ) -> Result<()> {
        require!(title.len() <= 64, UniVibeError::TitleTooLong);
        require!(metadata_uri.len() <= 128, UniVibeError::UriTooLong);
        require!(!milestone_amounts.is_empty(), UniVibeError::ZeroMilestones);
        require!(milestone_amounts.len() <= 10, UniVibeError::TooManyMilestones);
        require!(required_attestations > 0, UniVibeError::InvalidAttestationThreshold);

        let project = &mut ctx.accounts.project;
        project.authority = ctx.accounts.authority.key();
        project.beneficiary = ctx.accounts.beneficiary.key();
        project.project_id = project_id;
        project.title = title;
        project.metadata_uri = metadata_uri;
        project.milestone_amounts = milestone_amounts.clone();
        project.current_milestone = 0;
        project.total_milestones = milestone_amounts.len() as u8;
        project.required_attestations = required_attestations;
        project.is_completed = false;
        project.vault_bump = ctx.bumps.vault;
        project.bump = ctx.bumps.project;

        let total_funding_target: u64 = milestone_amounts
            .iter()
            .try_fold(0u64, |acc, &x| acc.checked_add(x))
            .ok_or(UniVibeError::MathOverflow)?;

        project.target_amount = total_funding_target;
        project.funded_amount = 0;

        emit!(ProjectInitialized {
            project: project.key(),
            authority: project.authority,
            beneficiary: project.beneficiary,
            project_id,
            target_amount: total_funding_target,
            milestones_count: project.total_milestones,
        });

        Ok(())
    }

    /// Deposits funds into the Project PDA Vault.
    pub fn fund_endowment(ctx: Context<FundEndowment>, amount: u64) -> Result<()> {
        require!(amount > 0, UniVibeError::ZeroDeposit);

        let project = &mut ctx.accounts.project;
        require!(!project.is_completed, UniVibeError::ProjectAlreadyCompleted);

        // Perform safe transfer from sponsor to project vault PDA
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.sponsor.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
            },
        );
        transfer(cpi_context, amount)?;

        project.funded_amount = project
            .funded_amount
            .checked_add(amount)
            .ok_or(UniVibeError::MathOverflow)?;

        emit!(EndowmentFunded {
            project: project.key(),
            sponsor: ctx.accounts.sponsor.key(),
            amount,
            total_funded: project.funded_amount,
        });

        Ok(())
    }

    /// Student submits work deliverable proof for the current milestone.
    pub fn submit_milestone(ctx: Context<SubmitMilestone>, deliverable_uri: String) -> Result<()> {
        require!(deliverable_uri.len() <= 128, UniVibeError::UriTooLong);
        let project = &ctx.accounts.project;
        require!(!project.is_completed, UniVibeError::ProjectAlreadyCompleted);

        let milestone_record = &mut ctx.accounts.milestone_record;
        milestone_record.project = project.key();
        milestone_record.milestone_index = project.current_milestone;
        milestone_record.deliverable_uri = deliverable_uri.clone();
        milestone_record.attestation_count = 0;
        milestone_record.is_approved = false;
        milestone_record.bump = ctx.bumps.milestone_record;

        emit!(MilestoneSubmitted {
            project: project.key(),
            milestone_index: project.current_milestone,
            deliverable_uri,
        });

        Ok(())
    }

    /// Authorized faculty/mentor attests to the submitted milestone.
    pub fn attest_milestone(ctx: Context<AttestMilestone>, approved: bool) -> Result<()> {
        let project = &ctx.accounts.project;
        let milestone_record = &mut ctx.accounts.milestone_record;

        require!(!project.is_completed, UniVibeError::ProjectAlreadyCompleted);
        require!(!milestone_record.is_approved, UniVibeError::MilestoneAlreadyApproved);

        let attestation = &mut ctx.accounts.attestation;
        require!(!attestation.has_voted, UniVibeError::MentorAlreadyAttested);

        attestation.has_voted = true;
        attestation.approved = approved;
        attestation.mentor = ctx.accounts.mentor.key();
        attestation.bump = ctx.bumps.attestation;

        if approved {
            milestone_record.attestation_count = milestone_record
                .attestation_count
                .checked_add(1)
                .ok_or(UniVibeError::MathOverflow)?;

            if milestone_record.attestation_count >= project.required_attestations {
                milestone_record.is_approved = true;
            }
        }

        emit!(MilestoneAttested {
            project: project.key(),
            milestone_index: project.current_milestone,
            mentor: ctx.accounts.mentor.key(),
            approved,
            total_approvals: milestone_record.attestation_count,
            is_milestone_ready: milestone_record.is_approved,
        });

        Ok(())
    }

    /// Claims funds for an approved milestone and transfers SOL directly to the beneficiary.
    pub fn claim_milestone_funds(ctx: Context<ClaimFunds>) -> Result<()> {
        let project = &mut ctx.accounts.project;
        let milestone_record = &ctx.accounts.milestone_record;

        require!(!project.is_completed, UniVibeError::ProjectAlreadyCompleted);
        require!(milestone_record.is_approved, UniVibeError::MilestoneNotApproved);

        let current_idx = project.current_milestone as usize;
        let payout_amount = project.milestone_amounts[current_idx];

        let vault_balance = ctx.accounts.vault.to_account_info().lamports();
        require!(vault_balance >= payout_amount, UniVibeError::InsufficientVaultBalance);

        let project_id_bytes = project.project_id.to_le_bytes();
        let authority_key = project.authority;
        let seeds = &[
            b"vault",
            authority_key.as_ref(),
            project_id_bytes.as_ref(),
            &[project.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        **ctx.accounts.vault.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .vault
            .to_account_info()
            .lamports()
            .checked_sub(payout_amount)
            .ok_or(UniVibeError::MathOverflow)?;

        **ctx.accounts.beneficiary.to_account_info().try_borrow_mut_lamports()? = ctx
            .accounts
            .beneficiary
            .to_account_info()
            .lamports()
            .checked_add(payout_amount)
            .ok_or(UniVibeError::MathOverflow)?;

        // Advance milestone
        project.current_milestone = project
            .current_milestone
            .checked_add(1)
            .ok_or(UniVibeError::MathOverflow)?;

        if project.current_milestone >= project.total_milestones {
            project.is_completed = true;
        }

        emit!(FundsDisbursed {
            project: project.key(),
            beneficiary: ctx.accounts.beneficiary.key(),
            milestone_index: (project.current_milestone - 1),
            amount: payout_amount,
            is_fully_completed: project.is_completed,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(project_id: u64)]
pub struct InitializeProject<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + ProjectRegistry::MAX_SIZE,
        seeds = [b"project", authority.key().as_ref(), project_id.to_le_bytes().as_ref()],
        bump
    )]
    pub project: Account<'info, ProjectRegistry>,

    /// CHECK: Vault PDA custodying funds safely
    #[account(
        mut,
        seeds = [b"vault", authority.key().as_ref(), project_id.to_le_bytes().as_ref()],
        bump
    )]
    pub vault: SystemAccount<'info>,

    /// CHECK: Beneficiary receiving grants
    pub beneficiary: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FundEndowment<'info> {
    #[account(
        mut,
        seeds = [b"project", project.authority.as_ref(), project.project_id.to_le_bytes().as_ref()],
        bump = project.bump
    )]
    pub project: Account<'info, ProjectRegistry>,

    /// CHECK: Vault PDA
    #[account(
        mut,
        seeds = [b"vault", project.authority.as_ref(), project.project_id.to_le_bytes().as_ref()],
        bump = project.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    #[account(mut)]
    pub sponsor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SubmitMilestone<'info> {
    #[account(
        seeds = [b"project", project.authority.as_ref(), project.project_id.to_le_bytes().as_ref()],
        bump = project.bump,
        has_one = beneficiary @ UniVibeError::UnauthorizedBeneficiary
    )]
    pub project: Account<'info, ProjectRegistry>,

    #[account(
        init,
        payer = beneficiary,
        space = 8 + MilestoneRecord::MAX_SIZE,
        seeds = [
            b"milestone",
            project.key().as_ref(),
            project.current_milestone.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub milestone_record: Account<'info, MilestoneRecord>,

    #[account(mut)]
    pub beneficiary: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AttestMilestone<'info> {
    #[account(
        seeds = [b"project", project.authority.as_ref(), project.project_id.to_le_bytes().as_ref()],
        bump = project.bump
    )]
    pub project: Account<'info, ProjectRegistry>,

    #[account(
        mut,
        seeds = [
            b"milestone",
            project.key().as_ref(),
            project.current_milestone.to_le_bytes().as_ref()
        ],
        bump = milestone_record.bump
    )]
    pub milestone_record: Account<'info, MilestoneRecord>,

    #[account(
        init,
        payer = mentor,
        space = 8 + MentorAttestation::MAX_SIZE,
        seeds = [
            b"attestation",
            milestone_record.key().as_ref(),
            mentor.key().as_ref()
        ],
        bump
    )]
    pub attestation: Account<'info, MentorAttestation>,

    #[account(mut)]
    pub mentor: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimFunds<'info> {
    #[account(
        mut,
        seeds = [b"project", project.authority.as_ref(), project.project_id.to_le_bytes().as_ref()],
        bump = project.bump,
        has_one = beneficiary @ UniVibeError::UnauthorizedBeneficiary
    )]
    pub project: Account<'info, ProjectRegistry>,

    /// CHECK: Vault PDA verified via seeds
    #[account(
        mut,
        seeds = [b"vault", project.authority.as_ref(), project.project_id.to_le_bytes().as_ref()],
        bump = project.vault_bump
    )]
    pub vault: SystemAccount<'info>,

    #[account(
        seeds = [
            b"milestone",
            project.key().as_ref(),
            project.current_milestone.to_le_bytes().as_ref()
        ],
        bump = milestone_record.bump
    )]
    pub milestone_record: Account<'info, MilestoneRecord>,

    #[account(mut)]
    pub beneficiary: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// STATE ACCOUNTS & EVENTS
// -----------------------------------------------------------------------------

#[account]
pub struct ProjectRegistry {
    pub authority: Pubkey,          // 32
    pub beneficiary: Pubkey,        // 32
    pub project_id: u64,            // 8
    pub title: String,              // 4 + 64
    pub metadata_uri: String,       // 4 + 128
    pub milestone_amounts: Vec<u64>,// 4 + (10 * 8)
    pub target_amount: u64,         // 8
    pub funded_amount: u64,         // 8
    pub current_milestone: u8,      // 1
    pub total_milestones: u8,       // 1
    pub required_attestations: u8,  // 1
    pub is_completed: bool,         // 1
    pub vault_bump: u8,             // 1
    pub bump: u8,                   // 1
}

impl ProjectRegistry {
    pub const MAX_SIZE: usize = 32 + 32 + 8 + (4 + 64) + (4 + 128) + (4 + 80) + 8 + 8 + 1 + 1 + 1 + 1 + 1 + 1;
}

#[account]
pub struct MilestoneRecord {
    pub project: Pubkey,            // 32
    pub milestone_index: u8,        // 1
    pub deliverable_uri: String,    // 4 + 128
    pub attestation_count: u8,      // 1
    pub is_approved: bool,          // 1
    pub bump: u8,                   // 1
}

impl MilestoneRecord {
    pub const MAX_SIZE: usize = 32 + 1 + (4 + 128) + 1 + 1 + 1;
}

#[account]
pub struct MentorAttestation {
    pub mentor: Pubkey,             // 32
    pub has_voted: bool,            // 1
    pub approved: bool,             // 1
    pub bump: u8,                   // 1
}

impl MentorAttestation {
    pub const MAX_SIZE: usize = 32 + 1 + 1 + 1;
}

#[event]
pub struct ProjectInitialized {
    pub project: Pubkey,
    pub authority: Pubkey,
    pub beneficiary: Pubkey,
    pub project_id: u64,
    pub target_amount: u64,
    pub milestones_count: u8,
}

#[event]
pub struct EndowmentFunded {
    pub project: Pubkey,
    pub sponsor: Pubkey,
    pub amount: u64,
    pub total_funded: u64,
}

#[event]
pub struct MilestoneSubmitted {
    pub project: Pubkey,
    pub milestone_index: u8,
    pub deliverable_uri: String,
}

#[event]
pub struct MilestoneAttested {
    pub project: Pubkey,
    pub milestone_index: u8,
    pub mentor: Pubkey,
    pub approved: bool,
    pub total_approvals: u8,
    pub is_milestone_ready: bool,
}

#[event]
pub struct FundsDisbursed {
    pub project: Pubkey,
    pub beneficiary: Pubkey,
    pub milestone_index: u8,
    pub amount: u64,
    pub is_fully_completed: bool,
}

#[error_code]
pub enum UniVibeError {
    #[msg("Project title exceeds 64 characters.")]
    TitleTooLong,
    #[msg("URI metadata exceeds 128 characters.")]
    UriTooLong,
    #[msg("Project must have at least one milestone.")]
    ZeroMilestones,
    #[msg("Project milestones cannot exceed 10.")]
    TooManyMilestones,
    #[msg("Attestation threshold must be greater than zero.")]
    InvalidAttestationThreshold,
    #[msg("Arithmetic overflow detected.")]
    MathOverflow,
    #[msg("Deposit amount must be greater than zero.")]
    ZeroDeposit,
    #[msg("Project is already completed.")]
    ProjectAlreadyCompleted,
    #[msg("Signer is not the authorized beneficiary.")]
    UnauthorizedBeneficiary,
    #[msg("Milestone is already approved.")]
    MilestoneAlreadyApproved,
    #[msg("Mentor has already submitted attestation for this milestone.")]
    MentorAlreadyAttested,
    #[msg("Milestone has not reached attestation approval threshold.")]
    MilestoneNotApproved,
    #[msg("Insufficient lamport balance in vault.")]
    InsufficientVaultBalance,
}