# Chainlink VRF Flat Fee

## Current Mainnet Value

The Ethereum mainnet configuration charges 40 USDS for each accepted randomness
request:

```text
randomnessFlatFee = 40 USDS
raw 18-decimal value = 40000000000000000000
```

The flat fee reimburses the DAO-funded Chainlink VRF v2.5 subscription. It is
separate from the ciphernode service fee. The service margin does not apply to
it.

## Calculation

The calculation uses the
[Chainlink VRF v2.5 subscription billing formula](https://docs.chain.link/vrf/v2-5/billing):

```text
gas cost = gas price * (verification gas + callback gas)
VRF cost = gas cost * (1 + native payment premium)
```

The release estimate uses these inputs:

| Input                  |         Value | Reason                                                                                                    |
| ---------------------- | ------------: | --------------------------------------------------------------------------------------------------------- |
| Gas price              |       50 gwei | Chainlink's Ethereum request-cost example                                                                 |
| Verification gas       |       115,000 | Chainlink's Ethereum request-cost example                                                                 |
| Callback gas           |        95,000 | Chainlink's Ethereum request-cost example                                                                 |
| Native payment premium |           24% | Chainlink's Ethereum subscription rate                                                                    |
| ETH reference price    | 2,442.81 USDS | [Chainlink ETH/USD feed](https://data.chain.link/feeds/ethereum/mainnet/eth-usd), 2026-08-26 09:47:59 UTC |

```text
gas units       = 115,000 + 95,000
gas cost        = 50 gwei * 210,000 = 0.0105 ETH
VRF cost        = 0.0105 ETH * 1.24 = 0.01302 ETH
reference cost  = 0.01302 ETH * 2,442.81 USDS/ETH
                = 31.805386 USDS
configured fee  = 40 USDS
buffer          = 8.194614 USDS, or approximately 25.8%
```

The conversion assumes that 1 USDS is worth 1 USD. Governance must review the
fee if USDS moves materially away from that target.

The configured callback limit is 150,000 gas. Subscription billing uses the
actual callback gas, not the complete limit. The subscription must still hold
the larger maximum reservation that the coordinator calculates from the gas lane
and callback limit.

## Settlement

```text
accepted E3 request
    -> service fee enters refundable escrow
    -> 40 USDS randomness fee becomes a treasury pull claim

successful E3
    -> nodes and the protocol split only the service fee

randomness timeout
    -> requester receives all service fee escrow
    -> randomness fee stays charged
```

A late fulfillment can charge the subscription after the E3 reaches its
randomness deadline. For this reason, the timeout refund does not include the
flat randomness fee. If the Chainlink request reverts, the complete E3 request
also reverts and Interfold collects no fee.

## Governance Review

The fee is fixed in USDS, but Chainlink bills the subscription in the configured
payment asset. Therefore, the fee can over-recover or under-recover its actual
cost.

Review the value before each release and before requests resume after a long
pause. Also review it when one of these inputs changes materially:

- The selected Chainlink gas lane.
- The callback implementation or callback gas use.
- The Chainlink premium.
- The expected Ethereum gas price.
- The ETH-to-USDS reference price.
- A material USDS-to-USD deviation.

Update the fee token, expected decimals, service prices, and `randomnessFlatFee`
in one `setFeeAssetConfig()` transaction.
