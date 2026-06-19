use anchor_lang::prelude::*;

#[error_code]
pub enum DuelError {
    #[msg("Bet amount must be greater than zero")]
    InvalidBetAmount,
    #[msg("Duel is not in the expected state for this action")]
    InvalidState,
    #[msg("You cannot join your own duel")]
    CannotJoinOwnDuel,
    #[msg("This duel is private and reserved for a specific opponent")]
    NotInvitedOpponent,
    #[msg("Provided VRF force does not match the expected request seed")]
    BadForce,
    #[msg("Provided randomness account does not match this duel")]
    RandomnessMismatch,
    #[msg("Randomness is not fulfilled yet; try again in a moment")]
    RandomnessNotReady,
    #[msg("The 10-minute join window has not lapsed yet; only the creator may cancel early")]
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
    #[msg("Only the treasury admin may perform this action")]
    Unauthorized,
}
