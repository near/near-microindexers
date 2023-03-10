# indexer-balances

See the [full README for the project in the root](../README.md)

## Features

`indexer-balances` has a feature `rpc-sanity-check` introduced to be able to test out the alternative RPC services.

Enabling this feature will require to provide `RPC_SANITY_CHECK_URL` which is expected to point to the NEAR RPC address:
- https://mainnet.rpc.near.org
- https://testnet.rpc.near.org

While the `RPC_URL` should point to the experimental (alternative) RPC server.

If the feature is enabled the indexer performs an extra call to the RPC (`RPC_SANITY_CHECK_URL`)
and compares the results. It falls back to the `RPC_SANITY_CHECK_URL`'s result in case of mismatch.
The corresponding warning log is being emitted in this case.
