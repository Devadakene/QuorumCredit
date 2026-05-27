use crate::errors::ContractError;
use crate::types::{Config, DataKey, LoanRecord};
use soroban_sdk::{token, Address, Env, String, Vec};

pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        Err(ContractError::ContractPaused)
    } else {
        Ok(())
    }
}

pub fn require_positive_amount(_env: &Env, amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InsufficientFunds);
    }
    Ok(())
}

pub fn config(env: &Env) -> Config {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .expect("not initialized")
}

pub fn add_slash_balance(env: &Env, amount: i128) {
    if amount <= 0 {
        return;
    }
    let current: i128 = env
        .storage()
        .instance()
        .get(&DataKey::SlashTreasury)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::SlashTreasury, &(current + amount));
}

pub fn deduct_slash_balance(env: &Env, amount: i128) -> Result<(), ContractError> {
    let current: i128 = env
        .storage()
        .instance()
        .get(&DataKey::SlashTreasury)
        .unwrap_or(0);
    if current < amount {
        return Err(ContractError::InsufficientFunds);
    }
    env.storage()
        .instance()
        .set(&DataKey::SlashTreasury, &(current - amount));
    Ok(())
}

pub fn has_active_loan(env: &Env, borrower: &Address) -> bool {
    matches!(
        get_active_loan_record(env, borrower),
        Ok(loan) if loan.status == crate::types::LoanStatus::Active
    )
}

pub fn get_active_loan_record(env: &Env, borrower: &Address) -> Result<LoanRecord, ContractError> {
    let loan_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveLoan(borrower.clone()))
        .ok_or(ContractError::NoActiveLoan)?;
    env.storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)
}

pub fn get_latest_loan_record(env: &Env, borrower: &Address) -> Option<LoanRecord> {
    if let Some(loan_id) = env
        .storage()
        .persistent()
        .get(&DataKey::LatestLoan(borrower.clone()))
    {
        env.storage().persistent().get(&DataKey::Loan(loan_id))
    } else if let Ok(loan) = get_active_loan_record(env, borrower) {
        Some(loan)
    } else {
        None
    }
}

pub fn next_loan_id(env: &Env) -> u64 {
    let loan_id = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .expect("loan ID overflow");
    env.storage()
        .persistent()
        .set(&DataKey::LoanCounter, &loan_id);
    loan_id
}

pub fn register_borrower_if_needed(env: &Env, borrower: &Address) {
    if !env
        .storage()
        .persistent()
        .has(&DataKey::BorrowerRegistered(borrower.clone()))
    {
        env.storage().persistent().set(
            &DataKey::BorrowerRegistered(borrower.clone()),
            &env.ledger().timestamp(),
        );
    }
}

pub fn borrower_registration_time(env: &Env, borrower: &Address) -> u64 {
    env.storage()
        .persistent()
        .get(&DataKey::BorrowerRegistered(borrower.clone()))
        .unwrap_or(0)
}

pub fn require_allowed_token<'a>(
    env: &'a Env,
    addr: &Address,
) -> Result<token::Client<'a>, ContractError> {
    let cfg = config(env);
    if *addr == cfg.token || cfg.allowed_tokens.iter().any(|t| t == *addr) {
        Ok(token::Client::new(env, addr))
    } else {
        Err(ContractError::InvalidToken)
    }
}

pub fn is_zero_address(env: &Env, addr: &Address) -> bool {
    let zero_account = Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    let zero_contract = Address::from_string(&String::from_str(
        env,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
    ));
    addr == &zero_account || addr == &zero_contract
}

pub fn require_valid_address(env: &Env, addr: &Address) -> Result<(), ContractError> {
    if is_zero_address(env, addr) {
        Err(ContractError::ZeroAddress)
    } else {
        Ok(())
    }
}

pub fn require_valid_token(env: &Env, addr: &Address) -> Result<(), ContractError> {
    require_valid_address(env, addr)?;
    let client = token::Client::new(env, addr);
    let probe = env.current_contract_address();
    if client.try_balance(&probe).is_err() {
        return Err(ContractError::InvalidToken);
    }
    Ok(())
}

pub fn validate_admin_config(
    env: &Env,
    admins: &Vec<Address>,
    admin_threshold: u32,
) -> Result<(), ContractError> {
    if admins.is_empty() || admin_threshold == 0 || admin_threshold > admins.len() {
        return Err(ContractError::InvalidAdminThreshold);
    }
    for i in 0..admins.len() {
        let admin = admins.get(i).unwrap();
        require_valid_address(env, &admin)?;
        for j in 0..i {
            if admin == admins.get(j).unwrap() {
                panic!("duplicate admin");
            }
        }
    }
    Ok(())
}

pub fn require_admin_approval(env: &Env, admin_signers: &Vec<Address>) {
    let cfg = config(env);
    assert!(
        admin_signers.len() >= cfg.admin_threshold,
        "insufficient admin approvals"
    );
    for signer in admin_signers.iter() {
        assert!(
            cfg.admins.iter().any(|a| a == signer),
            "signer is not a registered admin"
        );
        signer.require_auth();
    }
}

pub fn validate_config_bps(config: &Config) -> bool {
    config.recovery_percentage <= 10_000
}

pub fn get_admins(env: &Env) -> Vec<Address> {
    config(env).admins
}

pub fn token_client(env: &Env) -> soroban_sdk::token::Client<'_> {
    let addr = config(env).token;
    soroban_sdk::token::Client::new(env, &addr)
}

pub fn token(env: &Env) -> soroban_sdk::token::Client<'_> {
    token_client(env)
}
