#![no_std]

pub mod admin;
pub mod contract;
pub mod errors;
pub mod fraud_detection;
pub mod governance;
pub mod helpers;
pub mod insurance;
pub mod liquidity_mining;
pub mod loan;
pub mod pagination;
pub mod reputation;
pub mod signature;
pub mod staking_derivatives;
pub mod types;
pub mod vouch;
pub mod vouch_snapshot;

pub use contract::{QuorumCreditContract, QuorumCreditContractClient};
pub use errors::ContractError;
pub use types::*;

#[cfg(test)]
mod slash_recovery_test;
#[cfg(test)]
mod slash_redistribution_test;
#[cfg(test)]
mod slash_immunity_test;
#[cfg(test)]
mod slash_reversal_test;
