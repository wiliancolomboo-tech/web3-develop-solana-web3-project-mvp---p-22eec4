use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("UniV1bE111111111111111111111111111111111111");

#[program]
pub mod univibe {
    use super::*;

    /// Initializes a new university event escrow vault with a pre-funded micro-grant/reward pool.
    pub fn initialize_event(
        ctx: Context<InitializeEvent>,
        event_id: String,
        reward_per_attendee: u64,
        max_attendees: u32,
    ) -> Result<()> {
        require!(
            event_id.as_bytes().len() <= 32,
            UniVibeError::EventIdTooLong
        );
        require!(
            reward_per_attendee > 0,
            UniVibeError::InvalidRewardAmount
        );
        require!(max_attendees > 0, UniVibeError::InvalidMaxAttendees);

        let total_required = reward_per_attendee
            .checked_mul(max_attendees as u64)
            .ok_or(UniVibeError::MathOverflow)?;

        // Transfer funding from organizer to the Event Vault PDA
        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            Transfer {
                from: ctx.accounts.organizer.to_account_info(),
                to: ctx.accounts.event_vault.to_account_info(),
            },
        );
        transfer(cpi_context, total_required)?;

        let vault = &mut ctx.accounts.event_vault;
        vault.organizer = ctx.accounts.organizer.key();
        vault.event_id = event_id.clone();
        vault.reward_per_attendee = reward_per_attendee;
        vault.max_attendees = max_attendees;
        vault.attendees_checked_in = 0;
        vault.is_active = true;
        vault.bump = ctx.bumps.event_vault;

        emit!(EventCreated {
            event_id,
            organizer: vault.organizer,
            reward_per_attendee,
            max_attendees,
            total_locked: total_required,
        });

        Ok(())
    }

    /// Verifies attendance, marks Soulbound claim, and transfers SOL reward from PDA vault.
    pub fn check_in_and_claim(
        ctx: Context<CheckInAndClaim>,
        event_id: String,
    ) -> Result<()> {
        let vault = &mut ctx.accounts.event_vault;

        require!(vault.is_active, UniVibeError::EventInactive);
        require!(
            vault.attendees_checked_in < vault.max_attendees,
            UniVibeError::EventCapacityReached
        );

        let reward = vault.reward_per_attendee;

        // Perform PDA-signed transfer of reward to attendee
        let organizer_key = vault.organizer;
        let event_id_bytes = event_id.as_bytes();
        let bump_bytes = [vault.bump];
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            organizer_key.as_ref(),
            event_id_bytes,
            &bump_bytes,
        ]];

        let cpi_accounts = Transfer {
            from: vault.to_account_info(),
            to: ctx.accounts.attendee.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        transfer(cpi_ctx, reward)?;

        // Record attendance state
        let record = &mut ctx.accounts.attendance_record;
        record.event_id = event_id.clone();
        record.attendee = ctx.accounts.attendee.key();
        record.claimed_at = Clock::get()?.unix_timestamp;
        record.bump = ctx.bumps.attendance_record;

        vault.attendees_checked_in = vault
            .attendees_checked_in
            .checked_add(1)
            .ok_or(UniVibeError::MathOverflow)?;

        emit!(AttendeeCheckedIn {
            event_id,
            attendee: record.attendee,
            claimed_amount: reward,
            timestamp: record.claimed_at,
        });

        Ok(())
    }

    /// Closes event post-gathering and sweeps residual funds back to organizer.
    pub fn close_event(ctx: Context<CloseEvent>, event_id: String) -> Result<()> {
        let vault = &mut ctx.accounts.event_vault;
        vault.is_active = false;

        let organizer_key = vault.organizer;
        let event_id_bytes = event_id.as_bytes();
        let bump_bytes = [vault.bump];
        let signer_seeds: &[&[&[u8]]] = &[&[
            b"vault",
            organizer_key.as_ref(),
            event_id_bytes,
            &bump_bytes,
        ]];

        let balance = vault.to_account_info().lamports();
        let rent_exempt = Rent::get()?.minimum_balance(vault.to_account_info().data_len());
        
        // Ensure there is residual fund to withdraw
        if balance > rent_exempt {
            let refund_amount = balance.saturating_sub(rent_exempt);
            let cpi_accounts = Transfer {
                from: vault.to_account_info(),
                to: ctx.accounts.organizer.to_account_info(),
            };
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            );
            transfer(cpi_ctx, refund_amount)?;
        }

        emit!(EventClosed {
            event_id,
            organizer: ctx.accounts.organizer.key(),
            total_attendees: vault.attendees_checked_in,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(event_id: String, reward_per_attendee: u64, max_attendees: u32)]
pub struct InitializeEvent<'info> {
    #[account(
        init,
        payer = organizer,
        space = 8 + EventVault::MAX_SIZE,
        seeds = [b"vault", organizer.key().as_ref(), event_id.as_bytes()],
        bump
    )]
    pub event_vault: Account<'info, EventVault>,

    #[account(mut)]
    pub organizer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(event_id: String)]
pub struct CheckInAndClaim<'info> {
    #[account(
        mut,
        seeds = [b"vault", event_vault.organizer.as_ref(), event_id.as_bytes()],
        bump = event_vault.bump
    )]
    pub event_vault: Account<'info, EventVault>,

    #[account(
        init,
        payer = attendee,
        space = 8 + AttendanceRecord::MAX_SIZE,
        seeds = [b"attendance", event_id.as_bytes(), attendee.key().as_ref()],
        bump
    )]
    pub attendance_record: Account<'info, AttendanceRecord>,

    #[account(mut)]
    pub attendee: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(event_id: String)]
pub struct CloseEvent<'info> {
    #[account(
        mut,
        has_one = organizer @ UniVibeError::UnauthorizedOrganizer,
        seeds = [b"vault", organizer.key().as_ref(), event_id.as_bytes()],
        bump = event_vault.bump
    )]
    pub event_vault: Account<'info, EventVault>,

    #[account(mut)]
    pub organizer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

// -----------------------------------------------------------------------------
// STATE DEFINITIONS
// -----------------------------------------------------------------------------

#[account]
pub struct EventVault {
    pub organizer: Pubkey,          // 32
    pub event_id: String,           // 4 + 32
    pub reward_per_attendee: u64,   // 8
    pub max_attendees: u32,         // 4
    pub attendees_checked_in: u32,  // 4
    pub is_active: bool,            // 1
    pub bump: u8,                   // 1
}

impl EventVault {
    pub const MAX_SIZE: usize = 32 + (4 + 32) + 8 + 4 + 4 + 1 + 1;
}

#[account]
pub struct AttendanceRecord {
    pub attendee: Pubkey,   // 32
    pub event_id: String,   // 4 + 32
    pub claimed_at: i64,    // 8
    pub bump: u8,           // 1
}

impl AttendanceRecord {
    pub const MAX_SIZE: usize = 32 + (4 + 32) + 8 + 1;
}

// -----------------------------------------------------------------------------
// EVENTS & ERRORS
// -----------------------------------------------------------------------------

#[event]
pub struct EventCreated {
    pub event_id: String,
    pub organizer: Pubkey,
    pub reward_per_attendee: u64,
    pub max_attendees: u32,
    pub total_locked: u64,
}

#[event]
pub struct AttendeeCheckedIn {
    pub event_id: String,
    pub attendee: Pubkey,
    pub claimed_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct EventClosed {
    pub event_id: String,
    pub organizer: Pubkey,
    pub total_attendees: u32,
}

#[error_code]
pub enum UniVibeError {
    #[msg("Event ID exceeds maximum allowed length of 32 characters.")]
    EventIdTooLong,
    #[msg("Reward per attendee must be greater than zero.")]
    InvalidRewardAmount,
    #[msg("Max attendees must be greater than zero.")]
    InvalidMaxAttendees,
    #[msg("Arithmetic overflow detected.")]
    MathOverflow,
    #[msg("This event has ended or is marked inactive.")]
    EventInactive,
    #[msg("Event attendance capacity has already been reached.")]
    EventCapacityReached,
    #[msg("Unauthorized: Caller is not the registered organizer.")]
    UnauthorizedOrganizer,
}