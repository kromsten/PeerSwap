#[cfg(test)]
mod tests {
    use std::vec;

    use cosmwasm_std::{Addr, Empty, coin, Coin, Uint128};
    use cw_multi_test::{App, ContractWrapper, Executor};
    use cw_utils::NativeBalance;

    use crate::{contract::{*}, msg::{QueryMsg, GetOTCsResponse, ExecuteMsg, NewOTC}};

    fn mock_app() -> App {
        App::default()
    }

    pub fn mint_native(app: &mut App, recipient: String, denom: String, amount: u128) {
        app.sudo(cw_multi_test::SudoMsg::Bank(
            cw_multi_test::BankSudo::Mint {
                to_address: recipient,
                amount: vec![coin(amount, denom)],
            },
        ))
        .unwrap();
    }

    pub fn init_main(app: &mut App) -> Addr {
        let code = ContractWrapper::new(execute, instantiate, query);
        let code_id = app.store_code(Box::new(code));
        let res = app
            .instantiate_contract(
                code_id,
                Addr::unchecked("owner"),
                &Empty {},
                &[],
                "Contract",
                None,
            );
        res.unwrap()
    }

    pub fn query_otcs(app: &App, addr: Addr) -> Result<GetOTCsResponse, cosmwasm_std::StdError>   {
        let res = app.wrap()
            .query_wasm_smart(addr, &QueryMsg::GetOtcs { include_expired: None, start_after: None, limit: None });
        res
    }

    pub fn query_native_balance(app: &App, addr: Addr, denom: String) -> Result<Coin, cosmwasm_std::StdError> {
        let res = app.wrap()
            .query_balance(addr, denom);
        res
    }


    #[test]
    fn init_contract() {
        let mut app = mock_app();
        let addr = init_main(&mut app);
        assert!(addr.into_string().len() > 0)
    }

    #[test]
    fn init_balance() {
        let mut app = mock_app();
        let alice = Addr::unchecked("alice");
        let token = String::from("token1");
        
        let balance = query_native_balance(&app, alice.clone(), token.clone()).unwrap();
        assert_eq!(balance.amount, Uint128::zero());

        let amount : u128 = 1_000_000;
        mint_native(&mut app, alice.clone().to_string(), token.clone(), amount.clone());
        
        let balance = query_native_balance(&app, alice.clone(), token.clone()).unwrap();
        assert_eq!(balance.amount, Uint128::from(amount));
    }  


    #[test]
    fn fund_in_custody() {
        let mut app = mock_app();
        let contract_address = init_main(&mut app);

        let alice = Addr::unchecked("alice");
        let token = String::from("token1");
        let token2 = String::from("token2");
        let amount : u128 = 10_000_000;

        mint_native(&mut app, alice.clone().to_string(), token.clone(), amount.clone());

        let to_sell : u128 = 4_000_000;
        let to_ask : u128 = 5_000_000;

        let _res = app.execute_contract(
            alice.clone(), 
            contract_address.clone(),
            &ExecuteMsg::Create( NewOTC {
                ask_balances: vec![
                    cw20::Balance::Native(
                        NativeBalance(
                            vec![
                                coin(
                                    to_ask.clone(), 
                                    token2.clone()
                                )
                            ]
                        )
                    )
                ],
                expires: None,
                user_info: None,
                description: None,
            }),
            &vec![coin(to_sell, token.clone())],
        )

        .unwrap();


        let balance = query_native_balance(&app, alice, token.clone()).unwrap();
        assert_eq!(balance.amount, Uint128::from(amount - to_sell));


        let otcs = query_otcs(&app, contract_address).unwrap();

        let (_, otc) = otcs.otcs[0].clone();

        assert_eq!(otc.sell_amount, Uint128::from(to_sell));
        assert_eq!(otc.sell_denom, Some(token));

        assert_eq!(otc.ask_for[0].amount, Uint128::from(to_ask));
        
        
    }


}