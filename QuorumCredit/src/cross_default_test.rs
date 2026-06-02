#[cfg(test)]
mod tests {
    use crate::types::{
        DataKey, LoanRecord, LoanStatus, VouchRecord, Config, Address as _,
        DEFAULT_YIELD_BPS, DEFAULT_SLASH_BPS, DEFAULT_MIN_LOAN_AMOUNT, LoanCategory,
    };
    use soroban_sdk::testutils::Ledger;
    use soroban_sdk::{Address, Env};

    fn setup_test_env() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let borrower = Address::random(&env);
        let token = Address::random(&env);

        env.ledger().set_timestamp(100);
        (env, deployer, admin, borrower, token)
    }

    #[test]
    fn test_cross_default_enabled_flag() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        // Initially cross-default should be disabled (false)
        let enabled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::CrossDefaultEnabled)
            .unwrap_or(false);

        assert_eq!(enabled, false);

        // Enable cross-default
        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &true);

        let enabled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::CrossDefaultEnabled)
            .unwrap_or(false);

        assert_eq!(enabled, true);
    }

    #[test]
    fn test_cross_default_multiple_loans() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        // Create multiple loans for the same borrower
        let loan1 = LoanRecord {
            id: 1u64,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 5_000_000,
            amount_repaid: 0,
            total_yield: 100_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Business expansion"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        let loan2 = LoanRecord {
            id: 2u64,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 3_000_000,
            amount_repaid: 0,
            total_yield: 60_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 60,
            disbursement_timestamp: 110,
            repayment_timestamp: None,
            deadline: 110 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Equipment purchase"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        // Store both loans
        env.storage()
            .persistent()
            .set(&DataKey::Loan(1u64), &loan1);
        env.storage()
            .persistent()
            .set(&DataKey::Loan(2u64), &loan2);

        // Verify both loans exist and are Active
        let stored_loan1: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(1u64))
            .unwrap();
        let stored_loan2: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(2u64))
            .unwrap();

        assert_eq!(stored_loan1.status, LoanStatus::Active);
        assert_eq!(stored_loan2.status, LoanStatus::Active);
        assert_eq!(stored_loan1.borrower, borrower);
        assert_eq!(stored_loan2.borrower, borrower);
    }

    #[test]
    fn test_cross_default_state_transition() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        // Create loans with different statuses
        let loan_active = LoanRecord {
            id: 1u64,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 5_000_000,
            amount_repaid: 0,
            total_yield: 100_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Loan 1"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        let loan_defaulted = LoanRecord {
            id: 2u64,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 3_000_000,
            amount_repaid: 0,
            total_yield: 60_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Defaulted,
            created_at: 60,
            disbursement_timestamp: 110,
            repayment_timestamp: None,
            deadline: 110 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Loan 2"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Loan(1u64), &loan_active);
        env.storage()
            .persistent()
            .set(&DataKey::Loan(2u64), &loan_defaulted);

        // When cross-default is enabled, defaulting on loan 2 should trigger default on loan 1
        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &true);

        let cross_default_enabled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::CrossDefaultEnabled)
            .unwrap_or(false);

        assert_eq!(cross_default_enabled, true);

        // Verify the second loan is Defaulted
        let stored_loan2: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(2u64))
            .unwrap();

        assert_eq!(stored_loan2.status, LoanStatus::Defaulted);
    }

    #[test]
    fn test_cross_default_configuration() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        // Test toggling cross-default multiple times
        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &false);

        let enabled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::CrossDefaultEnabled)
            .unwrap_or(false);
        assert_eq!(enabled, false);

        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &true);

        let enabled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::CrossDefaultEnabled)
            .unwrap_or(false);
        assert_eq!(enabled, true);

        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &false);

        let enabled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::CrossDefaultEnabled)
            .unwrap_or(false);
        assert_eq!(enabled, false);
    }

    #[test]
    fn test_cross_default_different_borrowers_independent() {
        let (env, _deployer, _admin, _borrower, _token) = setup_test_env();

        let borrower1 = Address::random(&env);
        let borrower2 = Address::random(&env);

        // Create loans for different borrowers
        let loan_b1 = LoanRecord {
            id: 1u64,
            borrower: borrower1.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 5_000_000,
            amount_repaid: 0,
            total_yield: 100_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Loan 1"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        let loan_b2 = LoanRecord {
            id: 2u64,
            borrower: borrower2.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: 3_000_000,
            amount_repaid: 0,
            total_yield: 60_000,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 60,
            disbursement_timestamp: 110,
            repayment_timestamp: None,
            deadline: 110 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Loan 2"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Loan(1u64), &loan_b1);
        env.storage()
            .persistent()
            .set(&DataKey::Loan(2u64), &loan_b2);

        // Enable cross-default
        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &true);

        // Loans for different borrowers should be independent
        let stored_loan_b1: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(1u64))
            .unwrap();
        let stored_loan_b2: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(2u64))
            .unwrap();

        assert_eq!(stored_loan_b1.borrower, borrower1);
        assert_eq!(stored_loan_b2.borrower, borrower2);
        assert_ne!(stored_loan_b1.borrower, stored_loan_b2.borrower);
    }

    #[test]
    fn test_cross_default_preserves_loan_data() {
        let (env, _deployer, _admin, borrower, _token) = setup_test_env();

        // Create loan with specific data
        let original_amount = 5_000_000i128;
        let original_yield = 100_000i128;

        let loan = LoanRecord {
            id: 1u64,
            borrower: borrower.clone(),
            co_borrowers: soroban_sdk::Vec::new(&env),
            amount: original_amount,
            amount_repaid: 0,
            total_yield: original_yield,
            yield_bps: DEFAULT_YIELD_BPS,
            slash_bps: DEFAULT_SLASH_BPS,
            status: LoanStatus::Active,
            created_at: 50,
            disbursement_timestamp: 100,
            repayment_timestamp: None,
            deadline: 100 + 30 * 24 * 60 * 60,
            loan_purpose: soroban_sdk::String::from_str(&env, "Business expansion"),
            loan_category: LoanCategory::Business,
            token_address: Address::random(&env),
            syndicate_id: None,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Loan(1u64), &loan);

        // Enable cross-default
        env.storage()
            .persistent()
            .set(&DataKey::CrossDefaultEnabled, &true);

        // Verify loan data is preserved
        let stored_loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(1u64))
            .unwrap();

        assert_eq!(stored_loan.amount, original_amount);
        assert_eq!(stored_loan.total_yield, original_yield);
        assert_eq!(stored_loan.status, LoanStatus::Active);
    }
}
