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

    #[error("Wrong denomination")]
    WrongDenom {},

    #[error("The offer doesn't exist or has been completed already")] 
    NotFound {},

    #[error("Cannot set approval that is already expired")]
    Expired {},

    // Add any other custom errors you like here.
    // Look at https://docs.rs/thiserror/1.0.21/thiserror/ for details.
}
