# 0.3.1

* Restructured `BitcreditBillWeb` to a more structured approach, separating `status`,
`data` and `participants` and adding the concept of `current_waiting_state`, to
have all data available, if the bill is in a waiting state.
    * Added the concept of `redeemed_funds_available` on `status`, to indicate if
    the caller has funds available (e.g. from a sale, or a paid bill)

# 0.3.0

* First version exposing a WASM API
