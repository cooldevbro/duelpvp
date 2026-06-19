use anchor_lang::prelude::*;
use anchor_lang::solana_program::hash::hashv;

use orao_solana_vrf::cpi::accounts::RequestV2;
use orao_solana_vrf::program::OraoVrf;
use orao_solana_vrf::state::{NetworkState, RandomnessAccountData};
use orao_solana_vrf::{RANDOMNESS_ACCOUNT_SEED, CONFIG_ACCOUNT_SEED};

pub mod errors;
pub mod state;

use errors::DuelError;
use state::*;

// Replace with your own program id after `anchor keys sync`.
declare_id!("Due1PvP1111111111111111111111111111111111111");

pub const HOUSE_FEE_BPS: u64 = 100; // 1.00%
pub const BPS_DENOMINATOR: u64 = 10_000;
pub const JOIN_TIMEOUT_SECONDS: i64 = 600; // 10 minutes -> unmatched refund
pub const DUEL_EXPIRY_SECONDS: i64 = 86_400; // 24h -> stuck-VRF refund safety net

#[program]
pub mod duelpvp {
    use super::*;

    // ---------------------------------------------------------------------
    // Treasury (house fee sink)
    // ---------------------------------------------------------------------

    pub fn initialize_treasury(ctx: Context<InitializeTreasury>) -> Result<()> {
        let t = &mut ctx.accounts.treasury;
        t.admin = ctx.accounts.admin.key();
        t.bump = ctx.bumps.treasury;
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
        move_lamports(
            &treasury_ai,
            &ctx.accounts.destination.to_account_info(),
            amount,
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------------
    // 1) Create a duel (public or private). Creator funds the escrow.
    // ---------------------------------------------------------------------
    pub fn create_duel(
        ctx: Context<CreateDuel>,
        game_id: u64,
        bet_lamports: u64,
        win_condition: WinCondition,
        required_opponent: Option<Pubkey>,
    ) -> Result<()> {
        require!(bet_lamports > 0, DuelError::InvalidBetAmount);
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
        duel.join_deadline = now
            .checked_add(JOIN_TIMEOUT_SECONDS)
            .ok_or(DuelError::MathOverflow)?;
        duel.expiry = now
            .checked_add(DUEL_EXPIRY_SECONDS)
            .ok_or(DuelError::MathOverflow)?;
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
    // 2) Join a duel. Opponent funds the escrow AND requests VRF randomness
    //    in the same transaction. After this the game is fully on-chain and
    //    needs no further player signatures — only `settle_duel`.
    // ---------------------------------------------------------------------
    pub fn join_duel(ctx: Context<JoinDuel>, _game_id: u64, force: [u8; 32]) -> Result<()> {
        let opponent_key = ctx.accounts.opponent.key();
        let duel_key = ctx.accounts.duel.key();

        let (bet, created_at) = {
            let duel = &ctx.accounts.duel;
            require!(duel.status == DuelStatus::Waiting, DuelError::InvalidState);
            require!(opponent_key != duel.creator, DuelError::CannotJoinOwnDuel);
            if let Some(required) = duel.required_opponent {
                require!(opponent_key == required, DuelError::NotInvitedOpponent);
            }
            (duel.bet_lamports, duel.created_at)
        };

        // The VRF seed is bound deterministically to this duel + opponent, so
        // neither party can pick a favorable seed and the client can derive the
        // exact same randomness account address off-chain.
        let expected = hashv(&[
            duel_key.as_ref(),
            opponent_key.as_ref(),
            &created_at.to_le_bytes(),
        ])
        .to_bytes();
        require!(force == expected, DuelError::BadForce);

        // Opponent's wager into escrow.
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

        // Request randomness from ORAO via CPI. The opponent pays the VRF fee.
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
    // 3) Settle. Permissionless: anyone (either player's front end, or a
    //    relayer) calls this once ORAO has fulfilled the request. Reads the
    //    verifiable randomness, rolls the dice, pays the winner (or refunds a
    //    tie). The front-end animation simply replays this on-chain result.
    // ---------------------------------------------------------------------
    pub fn settle_duel(ctx: Context<SettleDuel>, _game_id: u64) -> Result<()> {
        {
            let duel = &ctx.accounts.duel;
            require!(duel.status == DuelStatus::Rolling, DuelError::InvalidState);
            require!(
                ctx.accounts.random.key() == duel.randomness,
                DuelError::RandomnessMismatch,
            );
        }

        // Read fulfilled randomness from the ORAO request account.
        let randomness = read_fulfilled_randomness(&ctx.accounts.random)?;

        let duel = &mut ctx.accounts.duel;
        // Two independent d6 per player from distinct bytes of the 64-byte output.
        duel.creator_dice = [randomness[0] % 6 + 1, randomness[1] % 6 + 1];
        duel.opponent_dice = [randomness[2] % 6 + 1, randomness[3] % 6 + 1];

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

            let pot = duel.bet_lamports.checked_mul(2).ok_or(DuelError::MathOverflow)?;
            let fee = pot
                .checked_mul(HOUSE_FEE_BPS)
                .ok_or(DuelError::MathOverflow)?
                / BPS_DENOMINATOR;
            let win_amount = pot.checked_sub(fee).ok_or(DuelError::MathOverflow)?;

            let winner_ai = if creator_wins {
                ctx.accounts.creator.to_account_info()
            } else {
                ctx.accounts.opponent.to_account_info()
            };
            move_lamports(&duel_ai, &winner_ai, win_amount)?;
            move_lamports(&duel_ai, &ctx.accounts.treasury.to_account_info(), fee)?;
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
    // 4) Close / refund. Covers the three exit paths:
    //    - Waiting: nobody joined. Creator may reclaim any time; anyone may
    //      trigger the refund after the 10-minute join window. Bet + rent to
    //      creator via `close = creator`.
    //    - Rolling: opponent joined but ORAO never fulfilled. Only after the
    //      24h expiry: refund the opponent here, creator gets bet + rent via
    //      close. (Liveness safety net; should never fire in practice.)
    //    - Settled: only rent remains -> creator. Permissionless.
    // ---------------------------------------------------------------------
    pub fn close_duel(ctx: Context<CloseDuel>, _game_id: u64) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let caller = ctx.accounts.caller.key();

        let (status, bet, opponent_key, join_deadline, expiry, creator, game_id) = {
            let d = &ctx.accounts.duel;
            (
                d.status,
                d.bet_lamports,
                d.opponent,
                d.join_deadline,
                d.expiry,
                d.creator,
                d.game_id,
            )
        };

        let mut refunded = false;
        match status {
            DuelStatus::Settled => {}
            DuelStatus::Waiting => {
                if caller != creator {
                    require!(now > join_deadline, DuelError::JoinWindowActive);
                }
                refunded = true; // creator bet + rent exit via close = creator
            }
            DuelStatus::Rolling => {
                require!(now > expiry, DuelError::NotExpired);
                require!(
                    ctx.accounts.opponent.key() == opponent_key,
                    DuelError::Unauthorized
                );
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

/// Deserialize an ORAO randomness request account and return its 64-byte
/// fulfilled value, erroring if the quorum has not fulfilled it yet.
///
/// NOTE: `RandomnessAccountData::fulfilled()` is the accessor in
/// orao-solana-vrf 0.4.x; cross-check against the Russian-Roulette CPI example
/// (rust/examples/cpi) if you bump the SDK version.
fn read_fulfilled_randomness(account: &AccountInfo) -> Result<[u8; 64]> {
    let data = account.try_borrow_data()?;
    let parsed = RandomnessAccountData::try_deserialize(&mut &data[..])
        .map_err(|_| error!(DuelError::RandomnessNotReady))?;
    let r = parsed
        .fulfilled()
        .ok_or(error!(DuelError::RandomnessNotReady))?;
    Ok(*r)
}

/// Move lamports out of a program-owned account by direct balance arithmetic.
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
    pub system_program: Program<'info, System>,
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

    // --- ORAO VRF accounts ---
    /// CHECK: ORAO network config PDA, validated by the VRF program.
    #[account(
        mut,
        seeds = [CONFIG_ACCOUNT_SEED],
        bump,
        seeds::program = orao_solana_vrf::ID
    )]
    pub vrf_config: Account<'info, NetworkState>,
    /// CHECK: ORAO treasury, validated by the VRF program against config.
    #[account(mut)]
    pub vrf_treasury: UncheckedAccount<'info>,
    /// CHECK: randomness request PDA (created by the CPI). Address is derived
    /// from `force`; the VRF program enforces the seeds on creation.
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
    /// Permissionless trigger; pays only the tx fee.
    pub caller: Signer<'info>,
    /// CHECK: must equal duel.creator (has_one) — payout destination.
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: must equal duel.opponent — payout destination.
    #[account(mut, address = duel.opponent)]
    pub opponent: UncheckedAccount<'info>,
    #[account(
        mut,
        seeds = [b"treasury"],
        bump = treasury.bump,
    )]
    pub treasury: Account<'info, Treasury>,
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
    /// Permissionless trigger.
    pub caller: Signer<'info>,
    /// CHECK: must equal duel.creator (has_one) — refund + rent destination.
    #[account(mut)]
    pub creator: UncheckedAccount<'info>,
    /// CHECK: opponent refund destination; verified in-handler only on the
    /// Rolling path. On the Waiting path the client may pass the creator.
    #[account(mut)]
    pub opponent: UncheckedAccount<'info>,
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
// Events  (front end subscribes to these to drive the UI)
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
