# PeerSwap

Salutations from PeerSwap!
Everyone can conduct OTC transactions, trade tokens, engage in arbitrage, and even raise money on PeerSwap! Try out now and start trading!

The repository contains for currently deployed contract of PeerSwap contract.
### Roadmap
1. Upgrade the essential platform
2. Add assistance for NFT transactions and trading, IBC enabled
3. Support FIAT on-ramp and off-ramp for OTC Deal
4. Add "Bet on anything" feature
built with the Archway Network.

## Usage Examplers:

Query all otcs:
```
archwayd q wasm contract-state smart $OTC_ADDRESS '{ "get_otcs" : {} }
```

Create an otc offer with native/ibc token:
```
archwayd tx wasm execute $OTC_ADDRESS '{ "create" : { "ask_balances": [{ "cw20": { "address": $CW20_ADDRESS, "amount": "1000000" } } ]  } }' --from wallet --amount 1000000uconst
```
Create an otc offer with cw20 token:
```
# $BASE_64_CREATE_MSG =  <- to base64 -- { "create" : { "ask_balances": [{ "native": [{ "denom": "uconst", "amount": "1000000" }] }]  } 
archwayd tx wasm execute $CW20_ADDRESS '{ "send": { "contract": $OTC_ADDRESS, "amount": "1000000", msg: $BASE_64_CREATE_MSG   } }'   }' --from wallet
```

Swap messages follow the same principle
