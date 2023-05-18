use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Custom Error val: {val:?}")]
    CustomError { val: String },

    #[error("Invalid start time message: {message:?}")]
    InvalidStartTime { message: String },

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("You have already joined this round")]
    AlreadyJoined {},

    #[error("Round with the provided name already exists")]
    RoundAlreadyExists {},

    #[error("Round with the provided name already ended")]
    RoundAlreadyEnded {},

    #[error("Round with the provided name does not exist")]
    RoundDoesNotExist {},

    #[error("Round with the provided name has already started")]
    RoundAlreadyStarted {},

    #[error("Round stop time already passed")]
    RoundStopTimePassed {},

    #[error("Round stop time has not yet reached")]
    RoundStillInProgress {},

    #[error("At least one coin must be deposited")]
    NoCoinsSent {},

    #[error("A maximum of one coin can be deposited")]
    TooManyCoins {},

    #[error("The deposited denom is not supported")]
    DenomNotSupported {},

    #[error("You are not a participant in this round")]
    NotJoined {},

    #[error("You have already placed a bet in this round")]
    BetAlreadyPlaced {},

    #[error("You have no bet in the provided round")]
    BetNotFound {},

    #[error("Can't edit bet to set the same side as the existing one")]
    DuplicateBetSide {},

    #[error("You have already claimed your win from the privided round")]
    WinAlreadyClaimed {},

    #[error("Fees for this round have already been claimed")]
    FeesAlreadyClaimed {},

    #[error("You cannot claim win from the provided round because you lost")]
    YouLost {},

    #[error("There is insufficient balance in treasury pool to withdraw the required amount")]
    InsufficientTreasuryDenomBalance {},

    #[error("The provided denom does not exist in the treasury pool")]
    TreasuryDenomDoesNotExist {},
}
