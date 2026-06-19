use anchor_lang::prelude::*;

/// Win condition for a duel.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum WinCondition {
    HigherWins,
    LowerWins,
}

/// Lifecycle:  Waiting -> Rolling -> Settled
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum DuelStatus {
    Waiting,
    Rolling,
    Settled,
}

/// Escrow + game state. One PDA per duel.
/// Seeds: [b"duel", game_id.to_le_bytes(), creator]
#[account]
#[derive(InitSpace)]
pub struct Duel {
    pub game_id: u64,
    pub creator: Pubkey,
    pub opponent: Pubkey, // Pubkey::default() until someone joins
    pub required_opponent: Option<Pubkey>, // private duel target
    pub bet_lamports: u64,
    pub win_condition: WinCondition,
    pub status: DuelStatus,

    // randomness (ORAO VRF). `force` is the joiner-supplied request seed.
    pub force: [u8; 32],
    pub randomness: Pubkey,

    // outcome (set at settle)
    pub creator_dice: [u8; 2],
    pub opponent_dice: [u8; 2],
    pub winner: Pubkey, // default on tie
    pub is_tie: bool,

    // timing
    pub created_at: i64,
    pub join_deadline: i64, // unmatched refund becomes claimable after this
    pub expiry: i64,        // stuck-VRF refund safety net

    pub bump: u8,
}

/// Fee sink + admin controls. Seeds: [b"treasury"]
#[account]
#[derive(InitSpace)]
pub struct Treasury {
    pub admin: Pubkey,
    /// Circuit breaker: when true, no new duels can be created.
    pub paused: bool,
    /// Optional per-duel cap in lamports. 0 = no limit.
    pub max_bet_lamports: u64,
    pub bump: u8,
}
