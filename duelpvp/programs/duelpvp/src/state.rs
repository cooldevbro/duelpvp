use anchor_lang::prelude::*;

/// Win condition for a duel.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum WinCondition {
    /// Highest dice total wins the pot.
    HigherWins,
    /// Lowest dice total wins the pot.
    LowerWins,
}

/// Lifecycle of a duel.
///   Waiting -> Rolling -> Settled
/// Waiting:  created + funded by creator; no opponent yet.
/// Rolling:  opponent joined + funded; VRF requested, awaiting fulfillment.
/// Settled:  randomness consumed, winner paid (or tie refunded).
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum DuelStatus {
    Waiting,
    Rolling,
    Settled,
}

/// Escrow + game state for a single duel. One PDA per duel.
/// Seeds: [b"duel", game_id.to_le_bytes(), creator]
#[account]
#[derive(InitSpace)]
pub struct Duel {
    pub game_id: u64,
    pub creator: Pubkey,
    /// Set once an opponent joins. Pubkey::default() until then.
    pub opponent: Pubkey,
    /// If Some, only this pubkey may join (private duel via link).
    pub required_opponent: Option<Pubkey>,
    /// Per-player wager in lamports. Pot = 2 * bet_lamports.
    pub bet_lamports: u64,
    pub win_condition: WinCondition,
    pub status: DuelStatus,

    // --- randomness (ORAO VRF) ---
    /// VRF request seed (commitment), derived deterministically at join.
    pub force: [u8; 32],
    /// The ORAO randomness account this duel's outcome is bound to.
    pub randomness: Pubkey,

    // --- outcome (filled at settle) ---
    pub creator_dice: [u8; 2],
    pub opponent_dice: [u8; 2],
    pub winner: Pubkey, // Pubkey::default() on tie
    pub is_tie: bool,

    // --- timing ---
    pub created_at: i64,
    pub join_deadline: i64, // created_at + JOIN_TIMEOUT_SECONDS  (unmatched refund)
    pub expiry: i64,        // created_at + DUEL_EXPIRY_SECONDS    (stuck-VRF refund)

    pub bump: u8,
}

/// Treasury that accumulates the house fee. Seeds: [b"treasury"]
#[account]
#[derive(InitSpace)]
pub struct Treasury {
    pub admin: Pubkey,
    pub bump: u8,
}
