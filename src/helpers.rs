use crate::errors::ContractError;
use crate::types::{Config, DataKey, LoanRecord, LoanStatus};
use soroban_sdk::{token, Address, Env, String, Vec};

pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    if paused {
        return Err(ContractError::ContractPaused);
    }
    let cfg = config(env);
    if cfg.emergency_pause_enabled {
        return Err(ContractError::ContractPaused);
    }
    Ok(())
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

pub fn get_admins(env: &Env) -> Vec<Address> {
    config(env).admins
}

pub fn has_active_loan(env: &Env, borrower: &Address) -> bool {
    matches!(
        get_active_loan_record(env, borrower),
        Ok(loan) if loan.status == LoanStatus::Active
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

pub fn add_slash_balance(env: &Env, amount: i128) {
    let current: i128 = env
        .storage()
        .instance()
        .get(&DataKey::SlashTreasury)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::SlashTreasury, &(current + amount));
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
    if admins.is_empty() {
        return Err(ContractError::InvalidAdminThreshold);
    }
    if admin_threshold == 0 || admin_threshold > admins.len() {
        return Err(ContractError::InvalidAdminThreshold);
    }
    let admin_count = admins.len();
    for i in 0..admin_count {
        let admin = admins.get(i).unwrap();
        require_valid_address(env, &admin)?;
        for j in 0..i {
            let prior_admin = admins.get(j).unwrap();
            if admin == prior_admin {
                return Err(ContractError::InvalidAdminThreshold);
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

pub fn is_admin(env: &Env, addr: &Address) -> bool {
    config(env).admins.iter().any(|a| a == *addr)
}

/// Governance participant: registered admin or holder of the protocol token.
pub fn is_governance_participant(env: &Env, addr: &Address) -> bool {
    if is_admin(env, addr) {
        return true;
    }
    let cfg = config(env);
    let token = token::Client::new(env, &cfg.token);
    token.balance(addr) > 0
}

pub fn require_governance_participant(env: &Env, addr: &Address) -> Result<(), ContractError> {
    if is_governance_participant(env, addr) {
        Ok(())
    } else {
        Err(ContractError::NotGovernanceParticipant)
    }
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

pub fn loan_status(env: &Env, borrower: &Address) -> LoanStatus {
    if let Ok(loan) = get_active_loan_record(env, borrower) {
        return loan.status;
    }
    if let Some(loan) = get_latest_loan_record(env, borrower) {
        return loan.status;
    }
    LoanStatus::None
}

/// Calculate the effective slash rate in basis points for a given loan.
///
/// Priority order:
/// 1. If `loan_size_slash_enabled`, scale slash_bps linearly with loan size relative
///    to total staked collateral, clamped to `[slash_bps, loan_size_slash_max_bps]`.
/// 2. If `dynamic_slash_threshold`, adjust based on protocol health score.
/// 3. Otherwise return the static `slash_bps` from config.
///
/// When both flags are enabled, loan-size scaling is applied first, then the
/// dynamic health adjustment is applied on top of that result.
pub fn calculate_effective_slash_bps(env: &Env, loan_amount: i128, total_stake: i128) -> i128 {
    let cfg = config(env);
    let base = cfg.slash_bps;

    let after_loan_size = if cfg.loan_size_slash_enabled {
        calculate_loan_size_slash_bps(loan_amount, total_stake, base, cfg.loan_size_slash_max_bps)
    } else {
        base
    };

    if cfg.dynamic_slash_threshold {
        calculate_dynamic_slash_threshold_from_base(env, after_loan_size)
    } else {
        after_loan_size
    }
}

/// Scale slash rate linearly with loan size relative to total staked collateral.
///
/// Formula:
///   ratio = loan_amount / total_stake  (clamped to [0, 1])
///   slash = base_bps + (max_bps - base_bps) * ratio
///
/// - A loan equal to 0% of total stake → `base_bps` (minimum slash)
/// - A loan equal to 100%+ of total stake → `max_bps` (maximum slash)
/// - Anything in between is linearly interpolated
///
/// All values are in basis points (10_000 = 100%).
pub fn calculate_loan_size_slash_bps(
    loan_amount: i128,
    total_stake: i128,
    base_bps: i128,
    max_bps: i128,
) -> i128 {
    use crate::types::BPS_DENOMINATOR;

    if total_stake <= 0 || loan_amount <= 0 {
        return base_bps;
    }

    // ratio in BPS: how large the loan is relative to total stake (capped at 100%)
    let ratio_bps = if loan_amount >= total_stake {
        BPS_DENOMINATOR // 100%
    } else {
        loan_amount * BPS_DENOMINATOR / total_stake
    };

    let range = max_bps.saturating_sub(base_bps);
    let adjustment = range * ratio_bps / BPS_DENOMINATOR;
    base_bps + adjustment
}

/// Calculate the protocol health score (0–10_000 basis points).
///
/// Components:
/// - Initialization (30%): 3_000 bps if contract is initialized
/// - Pause state  (30%): 3_000 bps if contract is NOT paused
/// - Solvency     (40%): 0–4_000 bps based on token balance
///   * 0 balance          → 0 bps
///   * 1–10 XLM           → linear 0–2_000 bps
///   * 10–100 XLM         → linear 2_000–4_000 bps
///   * 100+ XLM           → full 4_000 bps
pub fn calculate_protocol_health_score(env: &Env) -> i128 {
    let mut score: i128 = 0;

    // Initialization component (3_000 bps)
    if env.storage().instance().has(&DataKey::Config) {
        score += 3_000;
    }

    // Pause state component (3_000 bps)
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);
    let emergency: bool = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .map(|c: Config| c.emergency_pause_enabled)
        .unwrap_or(false);
    if !paused && !emergency {
        score += 3_000;
    }

    // Solvency component (0–4_000 bps)
    // 1 XLM = 10_000_000 stroops
    const MIN_SOLVENCY_STROOPS: i128 = 10_000_000;   // 1 XLM
    const MID_SOLVENCY_STROOPS: i128 = 100_000_000;  // 10 XLM
    const MAX_SOLVENCY_STROOPS: i128 = 1_000_000_000; // 100 XLM

    let balance: i128 = env
        .storage()
        .instance()
        .get(&DataKey::SlashTreasury)
        .unwrap_or(0);

    let solvency_score = if balance <= 0 {
        0
    } else if balance < MIN_SOLVENCY_STROOPS {
        // 0–1 XLM: linear 0–1_000 bps
        balance * 1_000 / MIN_SOLVENCY_STROOPS
    } else if balance < MID_SOLVENCY_STROOPS {
        // 1–10 XLM: linear 1_000–2_000 bps
        1_000 + (balance - MIN_SOLVENCY_STROOPS) * 1_000 / (MID_SOLVENCY_STROOPS - MIN_SOLVENCY_STROOPS)
    } else if balance < MAX_SOLVENCY_STROOPS {
        // 10–100 XLM: linear 2_000–4_000 bps
        2_000 + (balance - MID_SOLVENCY_STROOPS) * 2_000 / (MAX_SOLVENCY_STROOPS - MID_SOLVENCY_STROOPS)
    } else {
        4_000
    };

    score += solvency_score;
    score
}

/// Calculate the dynamic slash threshold based on protocol health, starting from a given base.
///
/// - Health ≥ 80% (HEALTH_THRESHOLD_BPS): interpolate DOWN from base toward MIN_DYNAMIC_SLASH_BPS
/// - Health < 80%: interpolate UP from base toward MAX_DYNAMIC_SLASH_BPS
pub fn calculate_dynamic_slash_threshold_from_base(env: &Env, base_bps: i128) -> i128 {
    use crate::types::{BPS_DENOMINATOR, HEALTH_THRESHOLD_BPS, MAX_DYNAMIC_SLASH_BPS, MIN_DYNAMIC_SLASH_BPS};

    let health_score = calculate_protocol_health_score(env);

    if health_score >= HEALTH_THRESHOLD_BPS {
        // Healthy: reduce slash toward minimum
        let health_factor = (health_score - HEALTH_THRESHOLD_BPS) * BPS_DENOMINATOR
            / (BPS_DENOMINATOR - HEALTH_THRESHOLD_BPS);
        let reduction = base_bps.saturating_sub(MIN_DYNAMIC_SLASH_BPS) * health_factor / BPS_DENOMINATOR;
        (base_bps - reduction).max(MIN_DYNAMIC_SLASH_BPS)
    } else {
        // Unhealthy: increase slash toward maximum
        let stress_factor = (HEALTH_THRESHOLD_BPS - health_score) * BPS_DENOMINATOR / HEALTH_THRESHOLD_BPS;
        let increase = MAX_DYNAMIC_SLASH_BPS.saturating_sub(base_bps) * stress_factor / BPS_DENOMINATOR;
        (base_bps + increase).min(MAX_DYNAMIC_SLASH_BPS)
    }
}

/// Calculate the dynamic slash threshold using the static config slash_bps as the base.
/// Returns the effective slash rate in basis points.
pub fn calculate_dynamic_slash_threshold(env: &Env) -> i128 {
    let cfg = config(env);
    if !cfg.dynamic_slash_threshold {
        return cfg.slash_bps;
    }
    calculate_dynamic_slash_threshold_from_base(env, cfg.slash_bps)
}
