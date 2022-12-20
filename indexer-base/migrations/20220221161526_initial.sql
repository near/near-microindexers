-- update_reason options:
--     {
--         'TRANSACTION_PROCESSING',
--         'ACTION_RECEIPT_PROCESSING_STARTED',
--         'ACTION_RECEIPT_GAS_REWARD',
--         'RECEIPT_PROCESSING',
--         'POSTPONED_RECEIPT',
--         'UPDATED_DELAYED_RECEIPTS',
--         'VALIDATOR_ACCOUNTS_UPDATE',
--         'MIGRATION',
--         'RESHARDING'
--     }
CREATE TABLE account_changes
(
    account_id                 text           NOT NULL,
    block_timestamp            numeric(20, 0) NOT NULL,
    block_hash                 text           NOT NULL,
    caused_by_transaction_hash text,
    caused_by_receipt_id       text,
    update_reason              text           NOT NULL,
    nonstaked_balance          numeric(38, 0) NOT NULL,
    staked_balance             numeric(38, 0) NOT NULL,
    storage_usage              numeric(20, 0) NOT NULL,
    chunk_index_in_block       integer        NOT NULL,
    index_in_chunk             integer        NOT NULL,
    PRIMARY KEY (block_timestamp, chunk_index_in_block, index_in_chunk)
);
ALTER TABLE account_changes
    ADD CONSTRAINT account_changes_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
ALTER TABLE account_changes
    ADD CONSTRAINT account_changes_receipt_id_fk FOREIGN KEY (caused_by_receipt_id) REFERENCES action_receipts (receipt_id);
ALTER TABLE account_changes
    ADD CONSTRAINT account_changes_tx_hash_fk FOREIGN KEY (caused_by_transaction_hash) REFERENCES transactions (transaction_hash);
CREATE INDEX CONCURRENTLY account_changes_account_idx ON account_changes (account_id);
CREATE INDEX CONCURRENTLY account_changes_block_hash_idx ON account_changes (block_hash);
CREATE INDEX CONCURRENTLY account_changes_block_timestamp_idx ON account_changes (block_timestamp);
CREATE INDEX CONCURRENTLY account_changes_receipt_id_idx ON account_changes (caused_by_receipt_id);
CREATE INDEX CONCURRENTLY account_changes_tx_hash_idx ON account_changes (caused_by_transaction_hash);
CREATE INDEX CONCURRENTLY account_changes_update_reason_idx ON account_changes (update_reason);

-- action_kind options:
--      {
--         'CREATE_ACCOUNT',
--         'DEPLOY_CONTRACT',
--         'FUNCTION_CALL',
--         'TRANSFER',
--         'STAKE',
--         'ADD_KEY',
--         'DELETE_KEY',
--         'DELETE_ACCOUNT'
--      }
CREATE TABLE action_receipts__actions
(
    block_hash             text           NOT NULL,
    block_timestamp        numeric(20, 0) NOT NULL,
    receipt_id             text           NOT NULL,
    action_kind            text           NOT NULL,
    -- https://docs.aws.amazon.com/redshift/latest/dg/json-functions.html
    -- https://docs.aws.amazon.com/redshift/latest/dg/super-overview.html
    args                   jsonb          NOT NULL,
    predecessor_account_id text           NOT NULL,
    receiver_account_id    text           NOT NULL,
    chunk_index_in_block   integer        NOT NULL,
    index_in_chunk         integer        NOT NULL,
    PRIMARY KEY (block_timestamp, chunk_index_in_block, index_in_chunk)
);
ALTER TABLE action_receipts__actions
    ADD CONSTRAINT action_receipt_actions_receipt_fk FOREIGN KEY (receipt_id) REFERENCES action_receipts (receipt_id);
CREATE INDEX CONCURRENTLY actions_action_kind_idx ON action_receipts__actions (action_kind);
CREATE INDEX CONCURRENTLY actions_predecessor_idx ON action_receipts__actions (predecessor_account_id);
CREATE INDEX CONCURRENTLY actions_receiver_idx ON action_receipts__actions (receiver_account_id);
CREATE INDEX CONCURRENTLY actions_block_timestamp_idx ON action_receipts__actions (block_timestamp);
CREATE INDEX CONCURRENTLY actions_args_function_call_idx ON action_receipts__actions ((args - >> 'method_name')) WHERE action_kind = 'FUNCTION_CALL';
-- CREATE INDEX CONCURRENTLY actions_args_receiver_id_idx ON action_receipts__actions ((args -> 'args_json' ->> 'receiver_id')) WHERE action_kind = 'FUNCTION_CALL' AND (args ->> 'args_json') IS NOT NULL;
-- CREATE INDEX CONCURRENTLY actions_receiver_and_timestamp_idx ON action_receipts__actions (receiver_account_id, block_timestamp);

CREATE TABLE action_receipts__outputs
(
    block_hash           text           NOT NULL,
    block_timestamp      numeric(20, 0) NOT NULL,
    receipt_id           text           NOT NULL,
    output_data_id       text           NOT NULL,
    receiver_account_id  text           NOT NULL,
    chunk_index_in_block integer        NOT NULL,
    index_in_chunk       integer        NOT NULL,
    PRIMARY KEY (block_timestamp, chunk_index_in_block, index_in_chunk)
);
ALTER TABLE action_receipts__outputs
    ADD CONSTRAINT outputs_receipt_fk FOREIGN KEY (receipt_id) REFERENCES action_receipts (receipt_id);
CREATE INDEX CONCURRENTLY outputs_block_timestamp_idx ON action_receipts__outputs (block_timestamp);
CREATE INDEX CONCURRENTLY outputs_output_data_id_idx ON action_receipts__outputs (output_data_id);
CREATE INDEX CONCURRENTLY outputs_receipt_id_idx ON action_receipts__outputs (receipt_id);
CREATE INDEX CONCURRENTLY outputs_receiver_account_id_idx ON action_receipts__outputs (receiver_account_id);

CREATE TABLE action_receipts
(
    receipt_id                       text           NOT NULL,
    block_hash                       text           NOT NULL,
    chunk_hash                       text           NOT NULL,
    block_timestamp                  numeric(20, 0) NOT NULL,
    chunk_index_in_block             integer        NOT NULL,
    receipt_index_in_chunk           integer        NOT NULL, -- goes both through action and data receipts
    predecessor_account_id           text           NOT NULL,
    receiver_account_id              text           NOT NULL,
    originated_from_transaction_hash text           NOT NULL,
    signer_account_id                text           NOT NULL,
    signer_public_key                text           NOT NULL,
--     todo change logic with gas_price + gas_used
-- https://github.com/near/near-analytics/issues/19
    gas_price                        numeric(38, 0) NOT NULL,
    PRIMARY KEY (receipt_id)
);
ALTER TABLE action_receipts
    ADD CONSTRAINT action_receipts_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
ALTER TABLE action_receipts
    ADD CONSTRAINT action_receipts_chunk_hash_fk FOREIGN KEY (chunk_hash) REFERENCES chunks (chunk_hash);
ALTER TABLE action_receipts
    ADD CONSTRAINT action_receipts_transaction_hash_fk FOREIGN KEY (originated_from_transaction_hash) REFERENCES transactions (transaction_hash);
CREATE INDEX CONCURRENTLY action_receipts_block_hash_idx ON action_receipts (block_hash);
-- CREATE INDEX CONCURRENTLY action_receipts_chunk_hash_idx ON action_receipts (chunk_hash);
CREATE INDEX CONCURRENTLY action_receipts_block_timestamp_idx ON action_receipts (block_timestamp);
CREATE INDEX CONCURRENTLY action_receipts_predecessor_idx ON action_receipts (predecessor_account_id);
CREATE INDEX CONCURRENTLY action_receipts_receiver_idx ON action_receipts (receiver_account_id);
CREATE INDEX CONCURRENTLY action_receipts_transaction_hash_idx ON action_receipts (originated_from_transaction_hash);
CREATE INDEX CONCURRENTLY action_receipts_signer_idx ON action_receipts (signer_account_id);

CREATE TABLE blocks
(
    block_height      numeric(20, 0) NOT NULL,
    block_hash        text           NOT NULL,
    prev_block_hash   text           NOT NULL,
    block_timestamp   numeric(20, 0) NOT NULL,
    total_supply      numeric(38, 0) NOT NULL,
--     todo next_block_gas_price? https://github.com/near/near-analytics/issues/19
    gas_price         numeric(38, 0) NOT NULL,
    author_account_id text           NOT NULL,
    PRIMARY KEY (block_hash)
);
CREATE INDEX CONCURRENTLY blocks_height_idx ON blocks (block_height);
-- CREATE INDEX CONCURRENTLY blocks_prev_hash_idx ON blocks (prev_block_hash);
CREATE INDEX CONCURRENTLY blocks_timestamp_idx ON blocks (block_timestamp);

CREATE TABLE chunks
(
    block_timestamp   numeric(20, 0) NOT NULL,
    block_hash        text           NOT NULL,
    chunk_hash        text           NOT NULL,
    index_in_block    integer        NOT NULL,
    signature         text           NOT NULL,
    gas_limit         numeric(20, 0) NOT NULL,
    gas_used          numeric(20, 0) NOT NULL,
    author_account_id text           NOT NULL,
    PRIMARY KEY (chunk_hash)
);
ALTER TABLE chunks
    ADD CONSTRAINT chunks_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
CREATE INDEX CONCURRENTLY chunks_block_timestamp_idx ON chunks (block_timestamp);
CREATE INDEX CONCURRENTLY chunks_block_hash_idx ON chunks (block_hash);

CREATE TABLE data_receipts
(
    receipt_id                       text           NOT NULL,
    block_hash                       text           NOT NULL,
    chunk_hash                       text           NOT NULL,
    block_timestamp                  numeric(20, 0) NOT NULL,
    chunk_index_in_block             integer        NOT NULL,
    receipt_index_in_chunk           integer        NOT NULL, -- goes both through action and data receipts
    predecessor_account_id           text           NOT NULL,
    receiver_account_id              text           NOT NULL,
    originated_from_transaction_hash text           NOT NULL,
    data_id                          text           NOT NULL,
    data                             bytea,
    PRIMARY KEY (receipt_id)
);
ALTER TABLE data_receipts
    ADD CONSTRAINT data_receipts_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
ALTER TABLE data_receipts
    ADD CONSTRAINT data_receipts_chunk_hash_fk FOREIGN KEY (chunk_hash) REFERENCES chunks (chunk_hash);
ALTER TABLE data_receipts
    ADD CONSTRAINT data_receipts_tx_hash_fk FOREIGN KEY (originated_from_transaction_hash) REFERENCES transactions (transaction_hash);
CREATE INDEX CONCURRENTLY data_receipts_block_hash_idx ON data_receipts (block_hash);
-- CREATE INDEX CONCURRENTLY data_receipts_chunk_hash_idx ON data_receipts (chunk_hash);
CREATE INDEX CONCURRENTLY data_receipts_block_timestamp_idx ON data_receipts (block_timestamp);
CREATE INDEX CONCURRENTLY data_receipts_predecessor_idx ON data_receipts (predecessor_account_id);
CREATE INDEX CONCURRENTLY data_receipts_receiver_idx ON data_receipts (receiver_account_id);
CREATE INDEX CONCURRENTLY data_receipts_transaction_hash_idx ON data_receipts (originated_from_transaction_hash);

CREATE TABLE execution_outcomes__receipts
(
    block_hash           text           NOT NULL,
    block_timestamp      numeric(20, 0) NOT NULL,
    executed_receipt_id  text           NOT NULL,
    produced_receipt_id  text           NOT NULL,
    chunk_index_in_block integer        NOT NULL,
    index_in_chunk       integer        NOT NULL,
    PRIMARY KEY (block_timestamp, chunk_index_in_block, index_in_chunk)
);
ALTER TABLE execution_outcomes__receipts
    ADD CONSTRAINT eo_receipts_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
ALTER TABLE execution_outcomes__receipts
    ADD CONSTRAINT eo_receipts_receipt_id_fk FOREIGN KEY (executed_receipt_id) REFERENCES execution_outcomes (receipt_id);
CREATE INDEX CONCURRENTLY execution_receipts_timestamp_idx ON execution_outcomes__receipts (block_timestamp);
CREATE INDEX CONCURRENTLY execution_receipts_produced_receipt_idx ON execution_outcomes__receipts (produced_receipt_id);

-- status options:
--      {
--         'UNKNOWN',
--         'FAILURE',
--         'SUCCESS_VALUE',
--         'SUCCESS_RECEIPT_ID'
--      }
-- todo we want to store more data for this table and maybe for the others
CREATE TABLE execution_outcomes
(
    receipt_id           text           NOT NULL,
    block_hash           text           NOT NULL,
    block_timestamp      numeric(20, 0) NOT NULL,
    chunk_index_in_block integer        NOT NULL,
    index_in_chunk       integer        NOT NULL,
    gas_burnt            numeric(20, 0) NOT NULL,
    tokens_burnt         numeric(38, 0) NOT NULL,
    executor_account_id  text           NOT NULL,
    status               text           NOT NULL,
    PRIMARY KEY (receipt_id)
);
ALTER TABLE execution_outcomes
    ADD CONSTRAINT execution_outcomes_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
ALTER TABLE execution_outcomes
    ADD CONSTRAINT execution_outcomes_receipt_id_fk FOREIGN KEY (receipt_id) REFERENCES action_receipts (receipt_id);
CREATE INDEX CONCURRENTLY execution_outcomes_block_timestamp_idx ON execution_outcomes (block_timestamp);
CREATE INDEX CONCURRENTLY execution_outcomes_block_hash_idx ON execution_outcomes (block_hash);
CREATE INDEX CONCURRENTLY execution_outcomes_status_idx ON execution_outcomes (status);

-- status options:
--      {
--         'UNKNOWN',
--         'FAILURE',
--         'SUCCESS_VALUE',
--         'SUCCESS_RECEIPT_ID'
--      }
CREATE TABLE transactions
(
    transaction_hash                text           NOT NULL,
    block_hash                      text           NOT NULL,
    chunk_hash                      text           NOT NULL,
    block_timestamp                 numeric(20, 0) NOT NULL,
    chunk_index_in_block            integer        NOT NULL,
    index_in_chunk                  integer        NOT NULL,
    signer_account_id               text           NOT NULL,
    signer_public_key               text           NOT NULL,
    nonce                           numeric(20, 0) NOT NULL,
    receiver_account_id             text           NOT NULL,
    signature                       text           NOT NULL,
    status                          text           NOT NULL,
    converted_into_receipt_id       text           NOT NULL,
    receipt_conversion_gas_burnt    numeric(20, 0),
    receipt_conversion_tokens_burnt numeric(38, 0),
    PRIMARY KEY (transaction_hash)
);
ALTER TABLE transactions
    ADD CONSTRAINT transactions_block_hash_fk FOREIGN KEY (block_hash) REFERENCES blocks (block_hash);
ALTER TABLE transactions
    ADD CONSTRAINT transactions_chunk_hash_fk FOREIGN KEY (chunk_hash) REFERENCES chunks (chunk_hash);
CREATE INDEX CONCURRENTLY transactions_receipt_id_idx ON transactions (converted_into_receipt_id);
CREATE INDEX CONCURRENTLY transactions_block_hash_idx ON transactions (block_hash);
CREATE INDEX CONCURRENTLY transactions_block_timestamp_idx ON transactions (block_timestamp);
-- CREATE INDEX CONCURRENTLY transactions_chunk_hash_idx ON transactions (chunk_hash);
CREATE INDEX CONCURRENTLY transactions_signer_idx ON transactions (signer_account_id);
-- CREATE INDEX CONCURRENTLY transactions_signer_public_key_idx ON transactions (signer_public_key);
CREATE INDEX CONCURRENTLY transactions_receiver_idx ON transactions (receiver_account_id);
-- CREATE INDEX CONCURRENTLY transactions_sorting_idx ON transactions (block_timestamp, chunk_index_in_block, index_in_chunk);

CREATE TABLE _blocks_to_rerun
(
    block_height numeric(20, 0) NOT NULL,
    PRIMARY KEY (block_height)
);
