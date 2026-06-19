use anchor_lang::prelude::*;
use anchor_lang::solana_program::bpf_loader_upgradeable;

use orao_solana_vrf::cpi::accounts::RequestV2;
use orao_solana_vrf::program::OraoVrf;
use orao_solana_vrf::state::{NetworkState, RandomnessAccountData};
use orao_solana_vrf::{CONFIG_ACCOUNT_SEED, RANDOMNESS_ACCOUNT_SEED};

pub mod errors;
pub mod state;

use errors::DuelError;
use state::*;

declare_id!("8NkYNEeX6eUiNrK89cHfNmZoigaUCdi5NLGKgRFJ77oZ");

// On-chain security.txt. Explorers like Solscan parse this and show the project
// name, website, and contacts on the program page. Only built into the program
// binary, never the CPI/IDL artifacts.
#[cfg(not(feature = "no-entrypoint"))]
use solana_security_txt::security_txt;
#[cfg(not(feature = "no-entrypoint"))]
security_txt! {
    name: "DuelPvP",
    project_url: "https://duelpvp.fun",
    contacts: "link:https://x.com/DuelPVPfun",
    policy: "https://github.com/duelpvpfun/duelpvp",
    preferred_languages: "en",
    source_code: "https://github.com/duelpvpfun/duelpvp"
}

pub const HOUSE_FEE_BPS: u64 = 100; // 1.00%
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const DUEL_EXPIRY_SECONDS: i64 = 86_400; // stuck-VRF refund safety net

#[program]
pub mod duelpvp {
    use super::*;

    // ---------------------------------------------------------------------
    // Treasury + admin
    // ---------------------------------------------------------------------

    /// One-time setup. Gated to the program's upgrade authority (the deployer),
    /// so a front-runner cannot seize admin by calling this first.
    pub fn initialize_treasury(ctx: Context<InitializeTreasury>) -> Result<()> {
        // Verify the supplied ProgramData account really is this program's
        // upgradeable-loader ProgramData and that `admin` is its upgrade
        // authority. The seeds constraint already pins it to the canonical PDA;
        // here we additionally confirm the owning program and the authority.
        let program_data_ai = ctx.accounts.program_data.to_account_info();
        require!(
            program_data_ai.owner == &bpf_loader_upgradeable::ID,
            DuelError::Unauthorized
        );
        let program_data = ProgramData::try_deserialize(
            &mut &program_data_ai.try_borrow_data()?[..],
        )
        .map_err(|_| error!(DuelError::Unauthorized))?;
        require!(
            program_data.upgrade_authority_address == Some(ctx.accounts.admin.key()),
            DuelError::Unauthorized
        );

        let t = &mut ctx.accounts.treasury;
        t.admin = ctx.accounts.admin.key();
        t.paused = false;
        t.max_bet_lamports = 0;
        t.bump = ctx.bumps.treasury;
        Ok(())
    }

    pub fn set_paused(ctx: Context<AdminOnly>, paused: bool) -> Result<()> {
        ctx.accounts.treasury.paused = paused;
        Ok(())
    }

    pub fn set_max_bet(ctx: Context<AdminOnly>, max_bet_lamports: u64) -> Result<()> {
        ctx.accounts.treasury.max_bet_lamports = max_bet_lamports;
        Ok(())
    }

    pub fn withdraw_treasury(ctx: Context<WithdrawTreasury>, amount: u64) -> Result<()> {
        let treasury_ai = ctx.accounts.treasury.to_account_info();
        let rent_min = Rent::get()?.minimum_balance(treasury_ai.data_len());
        let available = treasury_ai
            .lamports()
            .checked_sub(rent_min)
            .ok_or(DuelError::TreasuryRentViolation)?;
        require!(amount <= available, DuelError::TreasuryRentViolation);
        move_lamports(&treasury_ai, &ctx.accounts.destination.to_account_info(), amount)?;
        Ok(())
    }

    // ---------------------------------------------------------------------
    // 1) Create a duel (public or private). Creator funds the escrow.
    //    The listing stays open indefinitely until someone joins or the
    //    creator cancels it (no join timeout).
    // ---------------------------------------------------------------------
    pub fn create_duel(
        ctx: Context<CreateDuel>,
        game_id: u64,
        bet_lamports: u64,
        win_condition: WinCondition,
        required_opponent: Option<Pubkey>,
    ) -> Result<()> {
        require!(bet_lamports > 0, DuelError::InvalidBetAmount);

        let t = &ctx.accounts.treasury;
        require!(!t.paused, DuelError::Paused);
        if t.max_bet_lamports > 0 {
            require!(bet_lamports <= t.max_bet_lamports, DuelError::BetTooLarge);
        }

        let now = Clock::get()?.unix_timestamp;

        let duel = &mut ctx.accounts.duel;
        duel.game_id = game_id;
        duel.creator = ctx.accounts.creator.key();
        duel.opponent = Pubkey::default();
        duel.required_opponent = required_opponent;
        duel.bet_lamports = bet_lamports;
        duel.win_condition = win_condition;
        duel.status = DuelStatus::Waiting;
        duel.force = [0u8; 32];
        duel.randomness = Pubkey::default();
        duel.creator_dice = [0u8; 2];
        duel.opponent_dice = [0u8; 2];
        duel.winner = Pubkey::default();
        duel.is_tie = false;
        duel.created_at = now;
        duel.expiry = now.checked_add(DUEL_EXPIRY_SECONDS).ok_or(DuelError::MathOverflow)?;
        duel.bump = ctx.bumps.duel;

        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.creator.to_account_info(),
                    to: ctx.accounts.duel.to_account_info(),
                },
            ),
            bet_lamports,
        )?;

        emit!(DuelCreated {
            game_id,
            creator: ctx.accounts.creator.key(),
            bet_lamports,
            required_opponent,
        });
        Ok(())
    }

    // ---------------------------------------------------------------------
    // 2) Join. Opponent funds escrow AND requests VRF in one tx.
    //    `force` is fresh random entropy generated by the JOINER at join time.
    //    Because it is unknown to anyone until this tx lands, no one can
    //    pre-create the randomness PDA (no join-griefing) and no one can learn
    //    the outcome in advance. The joiner cannot grind it either: ORAO's
    //    output for any seed is unpredictable without ORAO's keys.
    // ---------------------------------------------------------------------
    pub fn join_duel(ctx: Context<JoinDuel>, _game_id: u64, force: [u8; 32]) -> Result<()> {
        let opponent_key = ctx.accounts.opponent.key();
        let bet = {
            let duel = &ctx.accounts.duel;
            require!(duel.status == DuelStatus::Waiting, DuelError::InvalidState);
            require!(opponent_key != duel.creator, DuelError::CannotJoinOwnDuel);
            if let Some(required) = duel.required_opponent {
                require!(opponent_key == required, DuelError::NotInvitedOpponent);
            }
            require!(force != [0u8; 32], DuelError::BadForce);
            duel.bet_lamports
        };

        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.opponent.to_account_info(),
                    to: ctx.accounts.duel.to_account_info(),
                },
            ),
            bet,
        )?;

        let cpi_ctx = CpiContext::new(
            ctx.accounts.vrf.to_account_info(),
            RequestV2 {
                payer: ctx.accounts.opponent.to_account_info(),
                network_state: ctx.accounts.vrf_config.to_account_info(),
                treasury: ctx.accounts.vrf_treasury.to_account_info(),
                request: ctx.accounts.random.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            },
        );
        orao_solana_vrf::cpi::request_v2(cpi_ctx, force)?;

        let duel = &mut ctx.accounts.duel;
        duel.opponent = opponent_key;
        duel.force = force;
        duel.randomness = ctx.accounts.random.key();
        duel.status = DuelStatus::Rolling;

        emit!(DuelJoined {
            game_id: duel.game_id,
            opponent: opponent_key,
            randomness: duel.randomness,
        });
        Ok(())
    }

    // ---------------------------------------------------------------------
    // 3) Settle (permissionless). Consume randomness, roll, pay.
    // ---------------------------------------------------------------------
    pub fn settle_duel(ctx: Context<SettleDuel>, _game_id: u64) -> Result<()> {
        {
            let duel = &ctx.accounts.duel;
            require!(duel.status == DuelStatus::Rolling, DuelError::InvalidState);
            require!(
                ctx.accounts.random.key() == duel.randomness,
                DuelError::RandomnessMismatch
            );
        }

        let randomness = read_fulfilled_randomness(&ctx.accounts.random)?;

        let duel = &mut ctx.accounts.duel;
        // Unbiased d6: rejection sampling (skip bytes >= 252 so all faces equiprobable).
        let mut cur = 0usize;
        duel.creator_dice = [next_d6(&randomness, &mut cur), next_d6(&randomness, &mut cur)];
        duel.opponent_dice = [next_d6(&randomness, &mut cur), next_d6(&randomness, &mut cur)];

        let cs = duel.creator_dice[0] as u16 + duel.creator_dice[1] as u16;
        let os = duel.opponent_dice[0] as u16 + duel.opponent_dice[1] as u16;

        let creator_wins = match duel.win_condition {
            WinCondition::HigherWins => cs > os,
            WinCondition::LowerWins => cs < os,
        };
        let tie = cs == os;
        duel.status = DuelStatus::Settled;

        let duel_ai = duel.to_account_info();
        if tie {
            duel.is_tie = true;
            duel.winner = Pubkey::default();
            move_lamports(&duel_ai, &ctx.accounts.creator.to_account_info(), duel.bet_lamports)?;
            move_lamports(&duel_ai, &ctx.accounts.opponent.to_account_info(), duel.bet_lamports)?;
        } else {
            let winner = if creator_wins { duel.creator } else { duel.opponent };
            duel.winner = winner;

            // SCALABILITY: the 1% house fee is NOT sent to the treasury here.
            // Doing so would mark `treasury` as writable in every settle and
            // serialize all non-tie settles on one account (Solana write-locks
            // an account per slot). Instead we pay the winner `pot - fee` and
            // leave `fee` sitting in the duel escrow. The fee is swept to the
            // treasury later, on the cold `close_duel` path, which is naturally
            // spread across time and accounts. This keeps settles fully
            // parallel across thousands of concurrent duels.
            let pot = duel.bet_lamports.checked_mul(2).ok_or(DuelError::MathOverflow)?;
            let fee = pot.checked_mul(HOUSE_FEE_BPS).ok_or(DuelError::MathOverflow)? / BPS_DENOMINATOR;
            let win_amount = pot.checked_sub(fee).ok_or(DuelError::MathOverflow)?;

            let winner_ai = if creator_wins {
                ctx.accounts.creator.to_account_info()
            } else {
                ctx.accounts.opponent.to_account_info()
            };
            move_lamports(&duel_ai, &winner_ai, win_amount)?;
            // `fee` remains in escrow (escrow now holds exactly rent + fee).
        }

        emit!(DuelSettled {
            game_id: duel.game_id,
            winner: duel.winner,
            is_tie: tie,
            creator_dice: duel.creator_dice,
            opponent_dice: duel.opponent_dice,
        });
        Ok(())
    }

    // ---------------------------------------------------------------------
    // 4) Close / refund. Cold path — also where the accrued house fee is
    //    swept to the treasury (kept off the hot `settle` path for scale).
    //    - Settled: sweep the 1% fee left in escrow to the treasury; the rent
    //      returns to the creator via `close = creator`.
    //    - Waiting: only the creator can cancel (no opponent funds at stake),
    //      any time. Creator's bet + rent return via `close = creator`.
    //    - Rolling: only valid if VRF NEVER fulfilled AND past expiry. Refund
    //      the opponent's bet here; creator's bet + rent leave via
    //      `close = creator`, so both players are made whole. If the VRF IS
    //      fulfilled the duel must be SETTLED, not refunded (see race guard).
    // ---------------------------------------------------------------------
    pub fn close_duel(ctx: Context<CloseDuel>, _game_id: u64) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let caller = ctx.accounts.caller.key();

        let (status, bet, opponent_key, expiry, creator, randomness, game_id) = {
            let d = &ctx.accounts.duel;
            (d.status, d.bet_lamports, d.opponent, d.expiry, d.creator, d.randomness, d.game_id)
        };

        let mut refunded = false;
        match status {
            DuelStatus::Settled => {
                // Sweep the accrued house fee (everything above rent) to the
                // treasury, then `close = creator` returns the rent. For ties
                // this is zero. This is the ONLY place the treasury is written,
                // keeping non-tie settles fully parallel.
                let duel_ai = ctx.accounts.duel.to_account_info();
                let rent_min = Rent::get()?.minimum_balance(duel_ai.data_len());
                let fee = duel_ai.lamports().saturating_sub(rent_min);
                move_lamports(&duel_ai, &ctx.accounts.treasury.to_account_info(), fee)?;
            }
            DuelStatus::Waiting => {
                // Only the creator can cancel an unmatched duel. No opponent
                // funds are ever at stake here, so there is no need for a
                // timeout or a permissionless refund path.
                require!(caller == creator, DuelError::Unauthorized);
                refunded = true; // creator bet + rent leave via close = creator
            }
            DuelStatus::Rolling => {
                // RACE GUARD: if the randomness was already fulfilled, this duel
                // has a determined winner and MUST be settled, not refunded. A
                // loser could otherwise wait out `expiry` and refund to escape a
                // loss. Require the caller to pass the real randomness account
                // and prove it is NOT yet fulfilled before allowing a refund.
                require!(
                    ctx.accounts.random.key() == randomness,
                    DuelError::RandomnessMismatch
                );
                require!(
                    !is_randomness_fulfilled(&ctx.accounts.random),
                    DuelError::AlreadyFulfilled
                );
                require!(now > expiry, DuelError::NotExpired);
                require!(ctx.accounts.opponent.key() == opponent_key, DuelError::Unauthorized);
                // Refund opponent's bet; creator's bet + rent leave via close = creator.
                move_lamports(
                    &ctx.accounts.duel.to_account_info(),
                    &ctx.accounts.opponent.to_account_info(),
                    bet,
                )?;
                refunded = true;
            }
        }

        emit!(DuelClosed { game_id, refunded });
        Ok(())
    }
}

// =========================================================================
// Helpers
// =========================================================================

/// Next unbiased d6 (1..=6) from the randomness buffer using rejection sampling.
fn next_d6(bytes: &[u8; 64], cursor: &mut usize) -> u8 {
    while *cursor < bytes.len() {
        let b = bytes[*cursor];
        *cursor += 1;
        if b < 252 {
            return b % 6 + 1;
        }
    }
    // Astronomically unlikely fallback (all remaining bytes >= 252).
    bytes[0] % 6 + 1
}

/// Read the 64-byte fulfilled VRF value, erroring if not yet fulfilled.
/// On orao-solana-vrf 0.4.0 the `RandomnessAccountData` enum exposes
/// `fulfilled_randomness()` (handles both V1 and V2 layouts). The `.fulfilled()`
/// accessor only exists on the inner `RandomnessV2`. Re-verify this accessor
/// against the installed ORAO version (russian-roulette example) if you bump it.
fn read_fulfilled_randomness(account: &AccountInfo) -> Result<[u8; 64]> {
    let data = account.try_borrow_data()?;
    let parsed = RandomnessAccountData::try_deserialize(&mut &data[..])
        .map_err(|_| error!(DuelError::RandomnessNotReady))?;
    let r = parsed
        .fulfilled_randomness()
        .ok_or(error!(DuelError::RandomnessNotReady))?;
    Ok(*r)
}

/// True if the ORAO randomness account exists and is already fulfilled.
/// Used by `close_duel` to forbid refunding a duel whose outcome is decided.
/// A non-existent / unparseable / pending account returns false (refund may
/// proceed once `expiry` has lapsed).
fn is_randomness_fulfilled(account: &AccountInfo) -> bool {
    // An uninitialized (system-owned, empty) account can never be fulfilled.
    if account.data_is_empty() {
        return false;
    }
    let Ok(data) = account.try_borrow_data() else {
        return false;
    };
    match RandomnessAccountData::try_deserialize(&mut &data[..]) {
        Ok(parsed) => parsed.fulfilled_randomness().is_some(),
        Err(_) => false,
    }
}

fn move_lamports(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    let mut from_l = from.try_borrow_mut_lamports()?;
    let mut to_l = to.try_borrow_mut_lamports()?;
    **from_l = from_l.checked_sub(amount).ok_or(DuelError::InsufficientFunds)?;
    **to_l = to_l.checked_add(amount).ok_or(DuelError::MathOverflow)?;
    Ok(())
}

// =========================================================================
// Account contexts
// =========================================================================

#[derive(Accounts)]
pub struct InitializeTreasury<'info> {
    // Must be the program's upgrade authority (the deployer). The authority
    // match is verified in the handler against the deserialized ProgramData.
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + Treasury::INIT_SPACE,
        seeds = [b"treasury"],
        bump
    )]
    pub treasury: Account<'info, Treasury>,
    /// CHECK: this program's ProgramData account. The canonical PDA is enforced
    /// by the seeds constraint below, and the handler verifies its owner is the
    /// upgradeable loader and that its upgrade_authority == admin. It is an
    /// `UncheckedAccount` (not `Account<ProgramData>`) only to avoid an IDL
    /// generator limitation with foreign anchor-lang account types; security is
    /// unchanged.
    #[account(
        seeds = [crate::ID.as_ref()],
        seeds::program = bpf_loader_upgradeable::ID,
        bump,
    )]
    pub program_data: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"treasury"],
        bump = treasury.bump,
        has_one = admin @ DuelError::Unauthorized,
    )]
    pub treasury: Account<'info, Treasury>,
}

#[derive(Accounts)]
pub struct WithdrawTreasury<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"treasury"],
        bump = treasury.bump,
        has_one = admin @ DuelError::Unauthorized,
    )]
    pub treasury: Account<'info, Treasury>,
    /// CHECK: arbitrary destination chosen by admin.
    #[account(mut)]
    pub destination: UncheckedAccount<'info>,
}

#[derive(Accounts)]
#[instruction(game_id: u64)]
pub struct CreateDuel<'info> {
    #[account(mut)]
    pub creator: Signer<'info>,
    #[account(
        init,
        payer = creator,
        space = 8 + Duel::INIT_SPACE,
        seeds = [b"duel", game_id.to_le_bytes().as_ref(), creator.key().as_ref()],
        bump
    )]
    pub duel: Account<'info, Duel>,
    // Read-only: enforces pause + max-bet. Must be initialized first.
    #[account(seeds = [b"treasury"], bump = treasury.bump)]
    pub treasury: Account<'info, Treasury>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: u64, force: [u8; 32])]
pub struct JoinDuel<'info> {
    #[account(mut)]
    pub opponent: Signer<'info>,
    /// CHECK: validated as the duel's creator via has_one + seed.
    pub creator: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"duel", game_id.to_le_bytes().as_ref(), creator.key().as_ref()],
        bump = duel.bump,
        has_one = creator,
    )]
    pub duel: Account<'info, Duel>,

    // ORAO VRF
    #[account(
        mut,
        seeds = [CONFIG_ACCOUNT_SEED],
        bump,
        seeds::program = orao_solana_vrf::ID
    )]
    pub vrf_config: Account<'info, NetworkState>,
    /// CHECK: ORAO treasury, validated by the VRF program against its config.
    #[account(mut)]
    pub vrf_treasury: UncheckedAccount<'info>,
    /// CHECK: randomness request PDA created by the CPI; seeds enforced by ORAO.
    #[account(
        mut,
        seeds = [RANDOMNESS_ACCOUNT_SEED, &force],
        bump,
        seeds::program = orao_solana_vrf::ID
    )]
    pub random: UncheckedAccount<'info>,
    pub vrf: Program<'info, OraoVrf>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(game_id: u64)]
pub struct SettleDuel<'info> {
    pub caller: Signer<'info>,
    /// CHECK: must equal duel.creator (has_one) — payout destination.
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: must equal duel.opponent — payout destination.
    #[account(mut, address = duel.opponent)]
    pub opponent: UncheckedAccount<'info>,
    // NOTE: the treasury is intentionally NOT in this context. Settle does not
    // touch the treasury (the fee stays in escrow and is swept at close), so
    // non-tie settles never write-lock a shared account and run fully parallel.
    #[account(
        mut,
        seeds = [b"duel", game_id.to_le_bytes().as_ref(), creator.key().as_ref()],
        bump = duel.bump,
        has_one = creator,
    )]
    pub duel: Account<'info, Duel>,
    /// CHECK: ORAO randomness account; checked == duel.randomness in handler.
    pub random: UncheckedAccount<'info>,
}

#[derive(Accounts)]
#[instruction(game_id: u64)]
pub struct CloseDuel<'info> {
    pub caller: Signer<'info>,
    /// CHECK: must equal duel.creator (has_one) — refund + rent destination.
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: opponent refund destination; verified in-handler on the Rolling
    /// path. On the Waiting path the client may pass the creator.
    #[account(mut)]
    pub opponent: UncheckedAccount<'info>,
    // Treasury receives the accrued 1% fee on the Settled path. Canonical PDA.
    #[account(mut, seeds = [b"treasury"], bump = treasury.bump)]
    pub treasury: Account<'info, Treasury>,
    /// CHECK: ORAO randomness account. On the Rolling path the handler requires
    /// this to equal duel.randomness and to be UNFULFILLED before refunding.
    /// On other paths it is unused (client may pass the duel's randomness key).
    pub random: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"duel", game_id.to_le_bytes().as_ref(), creator.key().as_ref()],
        bump = duel.bump,
        has_one = creator,
        close = creator,
    )]
    pub duel: Account<'info, Duel>,
}

// =========================================================================
// Events
// =========================================================================

#[event]
pub struct DuelCreated {
    pub game_id: u64,
    pub creator: Pubkey,
    pub bet_lamports: u64,
    pub required_opponent: Option<Pubkey>,
}

#[event]
pub struct DuelJoined {
    pub game_id: u64,
    pub opponent: Pubkey,
    pub randomness: Pubkey,
}

#[event]
pub struct DuelSettled {
    pub game_id: u64,
    pub winner: Pubkey,
    pub is_tie: bool,
    pub creator_dice: [u8; 2],
    pub opponent_dice: [u8; 2],
}

#[event]
pub struct DuelClosed {
    pub game_id: u64,
    pub refunded: bool,
}
