# Indexer Accounts

Async Postgres-compatible solution to load the data from NEAR blockchain.
Based on [NEAR Lake Framework](https://github.com/near/near-lake-framework-rs).

See [Indexer Base](https://github.com/near/near-indexer-base#indexer-base) docs for all the explanations, installation guide, etc.

### What else do I need to know?

Indexer Accounts is the only indexer that modifies the existing data.  
While other indexers are append-only, Indexer Accounts updates the existing records with the deletion info.

`accounts` table in [Indexer For Explorer](https://github.com/near/near-indexer-for-explorer) stored only the first creation and last deletion of the account.  
This solution stores all the creations/deletions, so accounts may appear in the table more than once.
