use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Too many denoms")]
    TooManyDenoms {},

    #[error("The amount is too small")]
    TooSmall {},

    #[error("Can't ask and sell the same token")]
    SameToken {},

    #[error("Can't create an offer without tokens to ask")]
    NoAskTokens {},

    #[error("Can't create an offer with many tokens to give")]
    TooManyGiveTokens {},

    #[error("Wrong denomination")]
    WrongDenom {},

    #[error("The offer doesn't exist or has been completed already")] 
    NotFound {},

    #[error("Cannot set approval that is already expired")]
    Expired {},

    #[error("The contract has been paused")]
    Stopped {},

    #[error("Semver parsing error: {0}")]
    SemVer(String),

    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}


impl From<semver::Error> for ContractError {
    fn from(err: semver::Error) -> Self {
        Self::SemVer(err.to_string())
    }
}