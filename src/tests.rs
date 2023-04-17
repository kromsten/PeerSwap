#[cfg(test)]
mod tests {
    use crate::contract::{execute, instantiate, query};
    use crate::error::ContractError;
    use crate::msg::{NewOTC, NewOTCResponse, ExecuteMsg, InstantiateMsg, QueryMsg, GetOTCsResponse};

    use cosmwasm_std::testing::{
        mock_dependencies, mock_env, mock_info, mock_dependencies_with_balances, 
    };
    use cosmwasm_std::{coins, from_binary, DepsMut, Response, Uint128,  Coin, Deps, Api, Env};
    use cw20::Balance;
    use cw_utils::{NativeBalance, Expiration};


    fn sell_native_ask_native(deps : DepsMut, count: u32, expires: Option<Expiration>) {

        let sell_amount = 5000000;
        let sell_denom = "token_1";

        let ask_amount = 10000000;
        let ask_denom = "token_2";

        //let api = deps.api;
        

        let info = mock_info(
            "alice", 
            &coins(
            sell_amount.clone(), 
            sell_denom.clone()
        ));
        
        let msg = ExecuteMsg::Create(NewOTC {
            ask_balances: vec![Balance::Native(NativeBalance(coins(ask_amount.clone(), ask_denom.clone())))],
            expires,
            user_info: None,
            description: None,
        });
        

        let res = execute(deps, mock_env(), info, msg);
        match res {
            Ok(Response { 
                messages: _, 
                attributes, 
                events: _, 
                data, .. 
            }) => {
                let attr = attributes
                    .iter()
                    .find(|a| a.key == "method")
                    .unwrap();
        
                assert!(attr.value == "create_new_otc");

                let res : NewOTCResponse = from_binary(&data.unwrap()).unwrap();

                assert!(res.id == count);
                
                let info = res.otc;

                assert_eq!(info.ask_for[0].amount, Uint128::from(ask_amount));
                assert_eq!(info.ask_for[0].denom, Some(ask_denom.to_string()));

                assert_eq!(info.sell_native, true);
                assert_eq!(info.sell_amount, Uint128::from(sell_amount));
                assert_eq!(info.sell_denom, Some(sell_denom.to_string()));

                assert_eq!(info.expires, expires.unwrap_or_default() );

                // asserts other fields of OTCInfo struct
                assert_eq!(info.user_info, None);
                assert_eq!(info.description, None);

            }
            Err(ContractError::Expired {}) => {
                assert!(expires.is_some() && expires.unwrap().is_expired(&mock_env().block));
            },
            _ => {
                panic!("Unknown error")
            }
        }
    }


    fn query_otcs(
            deps: Deps,
            env: Env,
            include_expired: Option<bool>,
            limit: Option<u32>,
            start_after: Option<u32>,
        ) -> GetOTCsResponse {
        
        
        let res = query(deps, env, QueryMsg::GetOtcs {
            include_expired: include_expired,
            limit: limit,
            start_after: start_after,
        }).unwrap();
        let value: GetOTCsResponse = from_binary(&res).unwrap();
        value
    }


    fn print_response(res: &Response) {

        println!("Messages:");
        for msg in res.messages.iter() {
            println!("{:?}", msg);
        }
        println!("");

        println!("Attributes:");
        for attr in res.attributes.iter() {
            println!("{:?}", attr);
        }
        println!("");


        println!("Events:");
        for event in res.events.iter() {
            println!("{:?}", event);
        }
        println!("");

        println!("Data:");
        println!("{:?}", res.data);

    }
    

    #[test]
    fn can_instantiate() {
        let mut deps = mock_dependencies();

        let res = instantiate_contract(deps.as_mut());
        assert_eq!(0, res.messages.len());

        let owner = &res
            .attributes
            .iter()
            .find(|a| a.key == "owner")
            .unwrap()
            .value;
        assert_eq!("admin", owner);
    }

    #[test]
    fn can_create_and_swap_native() {
        let mut deps = mock_dependencies_with_balances(&[
            ("alice", &coins(5, "token_1")),
            ("bob", &coins(10, "token_2")),
        ]);

        let env = mock_env();

        let count = 0;

        instantiate_contract(deps.as_mut());
        
        
        assert_eq!(query_otcs(deps.as_ref(), env.clone(), None, None, None).otcs.len(), 0);


        sell_native_ask_native(deps.as_mut(), count.clone(), None);


        let otcs = query_otcs(deps.as_ref(), env.clone(), None, None, None).otcs;
        assert_eq!(otcs.len(), 1);

        let (id, otc) = &otcs[0];
        assert_eq!(id, &count);

        assert!(
            otc.ask_for[0].amount == Uint128::from(10_000_000 as u128 ) &&
            deps.api.addr_humanize(&otc.seller).unwrap() == "alice",
        );

        let right_info = mock_info("bob", &coins(10_000_000, "token_2"));
        let smaller_amount_info = mock_info("bob", &coins(1_000_000, "token_2"));
        let wrong_denom_info = mock_info("bob", &coins(10_000_000, "token_3"));
        let multiple_tokens_info = mock_info("bob", 
            &vec!(
                Coin { 
                    amount: Uint128::from(10 as u8), 
                    denom: "token_2".to_string() 
                }, 
                Coin { 
                    amount: Uint128::from(10 as u8), 
                    denom: "token_3".to_string() 
                }
            )
        );
        //let bigger_amount_info = mock_info("bob", &coins(100, "token_2"));
        let msg = ExecuteMsg::Swap { otc_id: count.clone() };

   

        let res = execute(deps.as_mut(), mock_env(), smaller_amount_info, msg.clone()).unwrap();
        print_response(&res);


        // sent from otc offer to buyer
        assert_eq!(res.attributes[2].value, "500000");

        // sent to sellet from buyer
        assert_eq!(res.attributes[4].value, "1000000");


        let otcs = query_otcs(deps.as_ref(), env.clone(), None, None, None).otcs;
        assert_eq!(otcs.len(), 1);

        let (id, otc) = &otcs[0];
        assert_eq!(id, &count);

        // reduced amount
        assert!(
            otc.ask_for[0].amount == Uint128::from(9_000_000 as u128 ) &&
            deps.api.addr_humanize(&otc.seller).unwrap() == "alice",
        );


        let res = execute(deps.as_mut(), mock_env(), wrong_denom_info, msg.clone()).unwrap_err();
        assert_eq!(res.to_string(), ContractError::WrongDenom {}.to_string());
       

        let res = execute(deps.as_mut(), mock_env(), multiple_tokens_info, msg.clone()).unwrap_err();
        assert_eq!(res.to_string(), ContractError::TooManyDenoms {}.to_string());


        let res = execute(deps.as_mut(), mock_env(), right_info, msg.clone()).unwrap();
        
        let attr = res.attributes
                .iter()
                .find(|a| a.key == "method")
                .unwrap();

        assert!(attr.value == "swap");


        /* Mock querier doesn't work. Figure a way around it
        
        let querier = deps.as_ref().querier;
        let alice_balance = querier.query_all_balances("alice").unwrap()[0].clone();
        let bob_balance = querier.query_all_balances("bob").unwrap()[0].clone();

        api.debug(&format!("Alice balance: {:?} {}", alice_balance.amount, alice_balance.denom));
        api.debug(&format!("Bob balance: {:?} {}", bob_balance.amount, bob_balance.denom));

        assert_eq!(alice_balance.amount, Uint128::from(0 as u8));
        assert_eq!(bob_balance.amount, Uint128::from(10 as u8)); 
        */

    }
   

    #[test]
    fn queries_work() {
        let mut deps = mock_dependencies();
        let mut env = mock_env();

        let mut count  = 0;
        instantiate_contract(deps.as_mut());
        
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, None, None).otcs.len(), count );

        sell_native_ask_native(deps.as_mut(), 0, None);
        count += 1;
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, None, None).otcs.len(), count);

        sell_native_ask_native(deps.as_mut(), 1, None);
        count += 1;

        sell_native_ask_native(deps.as_mut(), 2, None);
        count += 1;

        // normal
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, None, None).otcs.len(), count);

        // limit
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, Some(2), None).otcs.len(), 2);
        
        // offset
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, None, Some(0)).otcs.len(), 2);

        // limit and offset
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, Some(1), Some(0)).otcs.len(), 1);

        // limit and offset over
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, Some(3), Some(1)).otcs.len(), 1); 

        // offset all
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, Some(5), Some(count as u32)).otcs.len(), 0); 
        
        
        sell_native_ask_native(deps.as_mut(), count as u32, Some(Expiration::AtHeight(12_345+1)));
        count += 1;

        env.block.height = 12_345 + 2;

        // exclude expired
        assert_eq!(query_otcs(deps.as_ref(), env.clone(), None, None, None).otcs.len(), count - 1);

        // include expired
        assert_eq!(query_otcs(deps.as_ref(), env, Some(true), None, None).otcs.len(), count);


    }
   
    #[test]
    fn can_cancell() {
        let mut deps = mock_dependencies();
        let env = mock_env();
        instantiate_contract(deps.as_mut());
        sell_native_ask_native(deps.as_mut(), 0, None);

        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, None, None).otcs.len(), 1);

        let msg = ExecuteMsg::Cancel { otc_id: 0 };
        let info = mock_info("alice", &[]);
        let res = execute(deps.as_mut(), mock_env(), info, msg.clone()).unwrap();
        assert_eq!(res.attributes[0].value, "cancel");
        assert_eq!(query_otcs(deps.as_ref(),env.clone(), None, None, None).otcs.len(), 0);
    }



    fn instantiate_contract(deps: DepsMut) -> Response {
        let msg = InstantiateMsg {};
        let info = mock_info("admin", &[]);
        instantiate(deps, mock_env(), info, msg).unwrap()
    } 
}



