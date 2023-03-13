# indexer-balances

See the [full README for the project in the root](../README.md)

## EXPETIMENTAL

`indexer-balances` in this experimental branch requires `EXPERIMENTAL_RPC_URL` to be provided. This is made to experiment with an alternative RPC service.

`EXPERIMENTAL_RPC_URL` should point to experimental RPC server.

While the `RPC_URL` should point to the stable RPC server.

The indexer performs an extra call to the RPC (`EXPERIMENTAL_RPC_URL`)
and compares the results. It uses the `RPC_URL`'s result in case of a mismatch and uses the results from `EXPERIMENTAL_RPC_URL` if the results match.

The corresponding warning log is being emitted in this case.
