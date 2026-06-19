use anchor_lang::prelude::*;

#[error_code]
pub enum DuelError {
    #[msg("Bet amount must be greater than zero")]
    InvalidBetAmount,
    #[msg("Bet exceeds the configured maximum")]
    BetTooLarge,
    #[msg("New duels are currently paused")]
    Paused,
    #[msg("Duel is not in the expected state for this action")]
    InvalidState,
    #[msg("You cannot join your own duel")]
    CannotJoinOwnDuel,
    #[msg("This duel is private and reserved for a specific opponent")]
    NotInvitedOpponent,
    #[msg("Invalid VRF force (must be non-zero random entropy)")]
    BadForce,
    #[msg("Provided randomness account does not match this duel")]
    RandomnessMismatch,
    #[msg("Randomness is not fulfilled yet; try again in a moment")]
    RandomnessNotReady,
    #[msg("Randomness already fulfilled; this duel must be settled, not refunded")]
    AlreadyFulfilled,
    #[msg("The join window has not lapsed yet; only the creator may cancel early")]
    JoinWindowActive,
    #[msg("Duel has not expired yet")]
    NotExpired,
    #[msg("Duel cannot be closed in its current state")]
    NotClosable,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Insufficient funds in escrow")]
    InsufficientFunds,
    #[msg("Withdrawal would drop the treasury below rent exemption")]
    TreasuryRentViolation,
    #[msg("Not authorized for this action")]
    Unauthorized,
}
