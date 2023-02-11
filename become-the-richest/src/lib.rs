//! # A Concordium V1 smart contract
use concordium_std::*;
use core::fmt::Debug;

const MIN_RAISE_FACTOR: u64 = 10; // 10%

/// Your smart contract state.
#[derive(Serialize, SchemaType, Clone)]
pub struct State {
    /// richest account
    richest_account: Option<AccountAddress>,
    /// richest message
    richest_message: String,
    /// min raise in euro_cent
    minimum_raise: u64,
}

/// Type of the parameter to the `init` function
#[derive(Serialize, SchemaType)]
struct InitParameter {
    minimum_raise: u64,
    richest_message: String,
}

/// Init function that creates a new smart become_the_richest contract.
#[init(contract = "become_the_richest", parameter = "InitParameter")]
fn init<S: HasStateApi>(
    _ctx: &impl HasInitContext,
    _state_builder: &mut StateBuilder<S>,
) -> InitResult<State> {
    let parameter: InitParameter = _ctx.parameter_cursor().get()?;
    Ok(State {
        richest_account: None,
        richest_message: parameter.richest_message,
        minimum_raise: parameter.minimum_raise,
    })
}

/// `become_the_richest_address` function errors
#[derive(Debug, PartialEq, Eq, Clone, Reject, Serial, SchemaType)]
enum BecomeTheRichestErrors {
    ParseError,
    /// Raised when a contract tries to bid; Only accounts
    /// are allowed to be richest.
    OnlyAccount,
    /// Raised when new amount is lower than current richest address amount.
    AmountBelowCurrentRichest,
    /// Raised when a new bid amount is raising the current highest bid
    /// with less than the minimum raise.
    AmountBelowMinimumRaise,
}

/// Receive function for accounts to become richest
#[receive(
    contract = "become_the_richest",
    name = "become_the_richest",
    payable,
    mutable,
    error = "BecomeTheRichestErrors",
    parameter = "String"
)]
fn become_the_richest<S: HasStateApi>(
    ctx: &impl HasReceiveContext,
    host: &mut impl HasHost<State, StateApiType = S>,
    amount: Amount,
) -> Result<(), BecomeTheRichestErrors> {
    let msg: String = match ctx.parameter_cursor().get() {
        Ok(msg) => msg,
        Err(_) => bail!(BecomeTheRichestErrors::ParseError),
    };

    let state = host.state();

    // Ensure that only accounts can place a bid
    let sender_address = match ctx.sender() {
        Address::Contract(_) => bail!(BecomeTheRichestErrors::OnlyAccount),
        Address::Account(account_address) => account_address,
    };

    // Balance of the contract
    let balance: Amount = host.self_balance();

    // Balance of the contract before the call
    let prev_richest_amount: Amount = balance - amount;

    // Ensure that the new bid exceeds the highest bid so far
    ensure!(
        amount > prev_richest_amount,
        BecomeTheRichestErrors::AmountBelowCurrentRichest
    );

    // Calculate the difference between the previous bid and the new bid in CCD.
    let amount_difference: Amount = amount - prev_richest_amount;
    // Get the current exchange rate used by the chain
    let exchange_rates = host.exchange_rates();
    // Convert the CCD difference to EUR
    let euro_cent_difference = exchange_rates.convert_amount_to_euro_cent(amount_difference);
    // Ensure that the bid is at least the `minimum_raise` more than the previous
    // bid
    ensure!(
        euro_cent_difference >= state.minimum_raise,
        BecomeTheRichestErrors::AmountBelowMinimumRaise
    );

    if let Some(account_address) = host.state_mut().richest_account.replace(sender_address) {
        host.invoke_transfer(&account_address, prev_richest_amount)
            .unwrap_abort();
    }
    // new minimum raise to 10% of current richest address
    let euro_cent_richest = exchange_rates.convert_amount_to_euro_cent(amount);
    host.state_mut().minimum_raise = MIN_RAISE_FACTOR * euro_cent_richest / 100;
    host.state_mut().richest_message = msg;
    Ok(())
}

/// View function that returns the content of the state.
#[receive(contract = "become_the_richest", name = "view", return_value = "State")]
fn view<'b, S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &'b impl HasHost<State, StateApiType = S>,
) -> ReceiveResult<&'b State> {
    Ok(host.state())
}

/// ViewRichestAddress  function that returns the highest bid which is the balance of
/// the contract
#[receive(
    contract = "become_the_richest",
    name = "viewRichestAddress",
    return_value = "Address"
)]
fn view_richest_address<S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &impl HasHost<State, StateApiType = S>,
) -> ReceiveResult<Option<AccountAddress>> {
    Ok(host.state().richest_account)
}

/// ViewRichestAmount  function that returns the highest bid which is the balance of
/// the contract
#[receive(
    contract = "become_the_richest",
    name = "viewRichestAmount",
    return_value = "Amount"
)]
fn view_richest_amount<S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &impl HasHost<State, StateApiType = S>,
) -> ReceiveResult<Amount> {
    Ok(host.self_balance())
}

/// ViewRichestMessage  function that returns the highest bid which is the balance of
/// the contract
#[receive(
    contract = "become_the_richest",
    name = "viewRichestMessage",
    return_value = "String"
)]
fn view_richest_message<S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &impl HasHost<State, StateApiType = S>,
) -> ReceiveResult<String> {
    Ok(host.state().richest_message.to_string())
}

/// ViewRichestMessage  function that returns the highest bid which is the balance of
/// the contract
#[receive(
    contract = "become_the_richest",
    name = "viewMinimumRaise",
    return_value = "u64"
)]
fn view_minimun_amount<S: HasStateApi>(
    _ctx: &impl HasReceiveContext,
    host: &impl HasHost<State, StateApiType = S>,
) -> ReceiveResult<u64> {
    Ok(host.state().minimum_raise)
}

#[concordium_cfg_test]
mod tests {
    use super::*;
    use std::format;
    use std::sync::atomic::{AtomicU8, Ordering};
    use test_infrastructure::*;

    // consts
    // A counter for generating new accounts
    static ADDRESS_COUNTER: AtomicU8 = AtomicU8::new(1);
    const INIT_DESC: &str = "No one is the Richest yet....";
    const CTX_TIME: u64 = 1;

    //type ContractResult<A> = Result<A, Error>;

    fn create_parameter_bytes(parameter: &InitParameter) -> Vec<u8> {
        to_bytes(parameter)
    }

    fn parametrized_init_ctx(parameter_bytes: &[u8]) -> TestInitContext {
        let mut ctx = TestInitContext::empty();
        ctx.set_parameter(parameter_bytes);
        ctx
    }

    /// test become_the_richest function
    fn become_the_richest_test<'a>(
        host: &mut TestHost<State>,
        amount: Amount,
        current_smart_contract_balance: Amount,
        msg: String,
    ) -> (Result<(), BecomeTheRichestErrors>, AccountAddress) {
        // account
        let account = AccountAddress([ADDRESS_COUNTER.load(Ordering::SeqCst); 32]);
        ADDRESS_COUNTER.fetch_add(1, Ordering::SeqCst);

        let mut ctx = TestReceiveContext::empty();
        ctx.set_sender(Address::Account(account));
        ctx.set_owner(account);
        ctx.set_metadata_slot_time(Timestamp::from_timestamp_millis(CTX_TIME));
        let p = to_bytes(&msg);
        ctx.set_parameter(&p);
        // mock balance increment
        host.set_self_balance(amount + current_smart_contract_balance);

        // Invoking the bid function.
        let state_result = become_the_richest(&ctx, host, amount);
        return (state_result, account);
    }

    // to test a specific error is emited
    fn expect_error<E, T>(expr: Result<T, E>, err: E, msg: &str)
    where
        E: Eq + Debug,
        T: Debug,
    {
        let actual = expr.expect_err_report(msg);
        claim_eq!(actual, err);
    }

    #[concordium_test]
    /// Test that initializing the contract succeeds with some state.
    fn test_init() {
        let ip = InitParameter {
            richest_message: INIT_DESC.to_string(),
            minimum_raise: 100,
        };

        let parameter_bytes = create_parameter_bytes(&ip);
        let ctx = parametrized_init_ctx(&parameter_bytes);

        let mut state_builder = TestStateBuilder::new();

        let state_result = init(&ctx, &mut state_builder);
        state_result.expect_report("Contract initialization results in error");
    }

    #[concordium_test]
    fn test_become_the_richest() {
        // initial params
        let ip = InitParameter {
            richest_message: String::from("No one is the richest"),
            minimum_raise: 1, // 1 cent
        };

        // create ctx
        let parameter_bytes = create_parameter_bytes(&ip);
        let ctx = parametrized_init_ctx(&parameter_bytes);

        let mut state_builder = TestStateBuilder::new();

        // Initializing contract
        let initial_state = init(&ctx, &mut state_builder).expect("Initialization should pass");

        // 1 CCD = 0,01 EUR = 1 cent
        // 1 euro = 100 CCD = 100_000_000 mCCD
        let mut host = TestHost::new(initial_state, state_builder);
        host.set_exchange_rates(ExchangeRates {
            euro_per_energy: ExchangeRate::new_unchecked(1, 1),
            micro_ccd_per_euro: ExchangeRate::new_unchecked(100000000, 1),
        });

        // richest of all time list ...
        // 1. Augustus Caesar
        // 2. Mansa Musa I
        // 3. Andrew Carnegie
        // 4. John D. Rockefeller
        // 5. William The Conqueror

        // The current_smart_contract_balance before the invoke is 0.
        let amount = Amount::from_ccd(100); // 1 euro
        let (s, a) = become_the_richest_test(
            &mut host,
            amount,
            Amount::from_ccd(0),
            String::from("William The Conqueror 300 B$"),
        );
        s.expect("William not ok!");
        let r_addrs = host.state().richest_account;
        claim_eq!(r_addrs, Some(a), "Should be william addrs");
        claim_eq!(
            host.state().richest_message,
            String::from("William The Conqueror 300 B$"),
            "Should be william msg"
        );
        claim_eq!(host.self_balance(), amount, "Should be 100 ccd");

        // The current_smart_contract_balance before the invoke is 100
        let amount = Amount::from_ccd(200);
        let (s, a) = become_the_richest_test(
            &mut host,
            amount,
            Amount::from_ccd(100),
            String::from("John D. Rockefeller 320 B$"),
        );
        s.expect("Rockefeller not ok!");
        let r_ = host.state().richest_account;
        claim_eq!(r_, Some(a), "Should be John D. Rockefeller addrs");
        claim_eq!(
            host.state().richest_message,
            String::from("John D. Rockefeller 320 B$"),
            "Should be Rockefeller msg"
        );
        claim_eq!(host.self_balance(), amount, "Should be 200 ccd");

        // The current_smart_contract_balance before the invoke is 200
        let amount = Amount::from_ccd(150); // 2 euro
        let (s, _) = become_the_richest_test(
            &mut host,
            amount,
            Amount::from_ccd(200),
            String::from("Fail !!!$"),
        );
        expect_error(
            s,
            BecomeTheRichestErrors::AmountBelowCurrentRichest,
            "Should fail with below current richest",
        );
        // The current_smart_contract_balance before the invoke is 100
        let amount = Amount::from_ccd(200);
        let (s, _) = become_the_richest_test(
            &mut host,
            amount,
            Amount::from_ccd(200),
            String::from("Fail !!!$"),
        );
        expect_error(
            s,
            BecomeTheRichestErrors::AmountBelowCurrentRichest,
            "Should fail with below current richest",
        );

        // The current_smart_contract_balance before the invoke is 100
        let amount = Amount::from_ccd(210);
        let (s, _) = become_the_richest_test(
            &mut host,
            amount,
            Amount::from_ccd(200),
            String::from("Fail !!!$"),
        );
        expect_error(
            s,
            BecomeTheRichestErrors::AmountBelowMinimumRaise,
            "Should fail with below current richest",
        );
        // The current_smart_contract_balance before the invoke is 200
        let amount = Amount::from_ccd(300);
        let (s, a) = become_the_richest_test(
            &mut host,
            amount,
            Amount::from_ccd(200),
            String::from("Andrew Carnegie 400B$"),
        );

        s.expect("Carnegie not ok!");
        let r_ = host.state().richest_account;
        claim_eq!(r_, Some(a), "Should be John D. Rockefeller addrs");
        claim_eq!(
            host.state().richest_message,
            String::from("Andrew Carnegie 400B$"),
            "Should be Carnegie msg"
        );
        claim_eq!(host.self_balance(), amount, "Should be 300 ccd");
        claim_eq!(
            host.state().minimum_raise,
            300 * MIN_RAISE_FACTOR / 100,
            "Should be 30 ccd"
        );
    }

    #[concordium_test]
    fn test_views() {
        // initial params
        let ip = InitParameter {
            richest_message: String::from("No one is the richest"),
            minimum_raise: 1, // 1 cent
        };

        // create ctx
        let parameter_bytes = create_parameter_bytes(&ip);
        let ctx = parametrized_init_ctx(&parameter_bytes);

        let mut state_builder = TestStateBuilder::new();

        // Initializing contract
        let initial_state = init(&ctx, &mut state_builder).expect("Initialization should pass");

        // 1 CCD = 0,01 EUR = 1 cent
        // 1 euro = 100 CCD = 100_000_000 mCCD
        let mut host = TestHost::new(initial_state, state_builder);
        host.set_exchange_rates(ExchangeRates {
            euro_per_energy: ExchangeRate::new_unchecked(1, 1),
            micro_ccd_per_euro: ExchangeRate::new_unchecked(100000000, 1),
        });

        // The current_smart_contract_balance before the invoke is 0.
        let amount = Amount::from_ccd(1000); // 10 euro
        let raise = 1000 * MIN_RAISE_FACTOR / 100;
        let msg_str = "Mr XXX is the richest";
        let msg = String::from(msg_str);
        let (s, a) = become_the_richest_test(&mut host, amount, Amount::from_ccd(0), msg);
        s.expect("First richest failed cannot view");
        let r_addrs = host.state().richest_account;
        claim_eq!(r_addrs, Some(a), "Should be Mr XXXX addrs");
        claim_eq!(host.state().richest_message, msg_str, "Should be {msg_str}");
        claim_eq!(host.self_balance(), amount, "Should be 1000 ccd");
        claim_eq!(host.state().minimum_raise, raise, "Should be 100 ccd");

        // new account and ctx for views
        let n_account = AccountAddress([ADDRESS_COUNTER.load(Ordering::SeqCst); 32]);
        ADDRESS_COUNTER.fetch_add(1, Ordering::SeqCst);
        let mut n_ctx = TestReceiveContext::empty();
        n_ctx.set_sender(Address::Account(n_account));
        n_ctx.set_owner(n_account);
        n_ctx.set_metadata_slot_time(Timestamp::from_timestamp_millis(CTX_TIME));

        let w_r = view_richest_address(&n_ctx, &mut host);
        let val = w_r.unwrap();
        claim_eq!(
            val,
            Some(a),
            "Should return correct addrs expected {:?} got {:?}",
            a,
            val
        );

        let w_r = view_richest_amount(&n_ctx, &mut host);
        let val = w_r.unwrap();
        claim_eq!(
            val,
            amount,
            "Should return correct amount expected {:?} got {:?}",
            amount,
            val
        );

        let w_r = view_richest_message(&n_ctx, &mut host);
        let val = w_r.unwrap();
        claim_eq!(
            val,
            msg_str,
            "Should return correct message expected {} got {}",
            msg_str,
            val
        );

        let w_r = view_minimun_amount(&n_ctx, &mut host);
        let val = w_r.unwrap();
        claim_eq!(val, raise, "\n expected {} got {}\n", raise, val);
    }
}
