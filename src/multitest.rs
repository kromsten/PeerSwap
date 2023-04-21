#[cfg(test)]
mod tests {
    use std::{vec, ops::Add};

    use cosmwasm_std::{Addr, Empty, coin, Coin, Uint128, from_binary, testing::mock_dependencies, Api, Event, Decimal};
    use cw20::{Balance, };
    use cw_multi_test::{App, ContractWrapper, Executor};
    use cw_utils::{NativeBalance, Expiration};

    use crate::{contract::{*}, msg::{QueryMsg, GetOTCsResponse, ExecuteMsg, NewOTC, NewOTCResponse}, error::ContractError, state::{OTCInfo, AskFor}};


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

    pub fn init_cw20(app: &mut App) -> Addr {

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

    pub fn new_otc_with_nones(ask_balances: Vec<Balance>) -> NewOTC {
        NewOTC {
            ask_balances,
            expires: None,
            user_info: None,
            description: None,
        }
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

    pub fn create_new_otc_with_funds(
        app: &mut App, 
        contract_address: Addr, 
        otc_data: NewOTC,
        send_funds: &Vec<Coin>

    ) -> Result<NewOTCResponse, cosmwasm_std::StdError> {

        let alice = Addr::unchecked("alice");

        let res =app.execute_contract(
            alice.clone(),
            contract_address.clone(),
            &ExecuteMsg::Create( otc_data ),
            send_funds,
        ).unwrap();

        from_binary(&res.data.unwrap())
    }

    pub fn native_wrapper(amount: u128, denom: String) -> Vec<cw20::Balance> {
        vec![cw20::Balance::Native( NativeBalance( vec![ coin(amount, denom) ] ) )]
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
    fn create_otc_check_native_asks()  {

        let mut app = mock_app();
        let contract_address = init_main(&mut app);

        let alice = Addr::unchecked("alice");
        let token = String::from("token1");
        let token2 = String::from("token2");
        
        let amount : u128 = 10_000_000;
        let to_ask : u128 = 5_000_000;


        mint_native(&mut app, alice.clone().to_string(), token.clone(), amount.clone());


        let no_balances = new_otc_with_nones(vec![]);

        let no_coins = new_otc_with_nones(vec![
            cw20::Balance::Native(
                NativeBalance(
                    vec![]
                )
            )
        ]);

        let same_token = new_otc_with_nones(vec![
            cw20::Balance::Native(
                NativeBalance(
                    vec![
                        coin(
                            to_ask.clone(), 
                            token.clone()
                        )
                    ]
                )
            )
        ]);


        let normal = new_otc_with_nones(vec![
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
        ]);
        
        let no_balances_res = app.execute_contract(
            alice.clone(),
            contract_address.clone(),
            &ExecuteMsg::Create( no_balances ),
            &vec![coin(amount, token.clone())],
        ).unwrap_err();
        
        
        assert_eq!(
            no_balances_res.root_cause().to_string(), 
            ContractError::NoAskTokens {}.to_string()
        );


        let no_coins_res = app.execute_contract(
            alice.clone(),
            contract_address.clone(),
            &ExecuteMsg::Create( no_coins ),
            &vec![coin(amount, token.clone())],
        ).unwrap_err();

        assert_eq!(
            no_coins_res.root_cause().to_string(), 
            ContractError::NoAskTokens {}.to_string()
        );


        let same_token_res = app.execute_contract(
            alice.clone(),
            contract_address.clone(),
            &ExecuteMsg::Create( same_token ),
            &vec![coin(amount, token.clone())],
        ).unwrap_err();


        assert_eq!(
            same_token_res.root_cause().to_string(), 
            ContractError::SameToken {  }.to_string()
        );


        let normal_res = app.execute_contract(
            alice.clone(),
            contract_address.clone(),
            &ExecuteMsg::Create( normal.clone() ),
            &vec![coin(amount, token.clone())],
        ).unwrap();


        let data : NewOTCResponse = from_binary(&normal_res.data.unwrap()).unwrap();

        assert_eq!(
            data,
            NewOTCResponse {
                id: 0,
                otc: OTCInfo { 
                    seller: mock_dependencies().api.addr_canonicalize(alice.as_str()).unwrap(), 
                    sell_native: true, 
                    sell_amount: amount.clone().into(), 
                    initial_sell_amount: amount.into(), 
                    sell_denom: Some(token), 
                    sell_address: None, 
                    ask_for: vec![
                        AskFor {
                            address: None,
                            denom: Some(token2),
                            amount: to_ask.clone().into(),
                            initial_amount: to_ask.into(),
                            native: true
                        }
                    ], 
                    expires: Expiration::Never {}, 
                    user_info: normal.user_info, 
                    description: normal.description

                }
            }
        );



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
            &ExecuteMsg::Create(
                new_otc_with_nones(
                    native_wrapper(to_ask.clone(), token2.clone())
                )
            ),
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


    #[test]
    fn create_native_swap_native_full()  {

        let mut app = mock_app();
        let contract_address = init_main(&mut app);

        let alice = Addr::unchecked("alice");
        let bob = Addr::unchecked("bob");

        let token = String::from("token1");
        let token2 = String::from("token2");

        let amount : u128 = 10_000_000;
        let amount2 : u128 = 5_000_000;
        
        mint_native(
            &mut app, 
            alice.clone().to_string(), 
            token.clone(), 
            amount.clone()
        );

        mint_native(
            &mut app, 
            bob.clone().to_string(), 
            token2.clone(), 
            amount2.clone()
        );


        let _new_otc = create_new_otc_with_funds(
            &mut app, 
            contract_address.clone(), 
            new_otc_with_nones(
                native_wrapper(
                    amount2.clone(), 
                    token2.clone()
                )
            ),
            &vec![coin(amount.clone(), token.clone())],
            
        ).unwrap();
    
        
        let otcs = query_otcs(&app, contract_address.clone()).unwrap();
        assert_eq!(otcs.otcs.len(), 1);

        let (id, _) = otcs.otcs[0].clone();


        let res = app.execute_contract(
            bob.clone(), 
            contract_address.clone(), 
            &ExecuteMsg::Swap {
                otc_id: id,
            },
            &vec![
                coin(amount2.clone(), token2.clone())
            ]
        ).unwrap();


        let wasm_event = res.events[1].clone();

        let given_amount = &wasm_event.attributes[3];
        let given_token = &wasm_event.attributes[4];

        let sent_amount = &wasm_event.attributes[5];
        let sent_token = &wasm_event.attributes[6];

        let completed = &wasm_event.attributes[7];

        assert_eq!(given_amount.value, amount.clone().to_string());
        assert_eq!(given_token.value, token.clone());
        assert_eq!(sent_amount.value, amount2.clone().to_string());
        assert_eq!(sent_token.value, token2.clone());
        assert_eq!(completed.value, "true");

        let otcs = query_otcs(&app, contract_address.clone()).unwrap();
        assert_eq!(otcs.otcs.len(), 0);

        let owner = Addr::unchecked("owner");
        let maker_fee_rate = Decimal::from_ratio(2 as u8, 100 as u8);
        let taker_fee_rate = Decimal::from_ratio(1 as u8, 100 as u8);


        let maker_fee = Uint128::from(amount2) * maker_fee_rate;
        
        let balance = query_native_balance(&app, alice, token2.clone()).unwrap();
        assert_eq!(balance.amount, Uint128::from(amount2) - maker_fee);

        let balance = query_native_balance(&app, owner.clone(), token2.clone()).unwrap();
        assert_eq!(balance.amount, maker_fee);


        let taker_fee = Uint128::from(amount) * taker_fee_rate;

        let balance = query_native_balance(&app, bob, token.clone()).unwrap();
        assert_eq!(balance.amount, Uint128::from(amount) - taker_fee);

        let balance = query_native_balance(&app, owner, token.clone()).unwrap();
        assert_eq!(balance.amount, taker_fee);

    }



}