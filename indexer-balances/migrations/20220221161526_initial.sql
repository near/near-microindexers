CREATE TABLE near_balance_events
(
    event_index               numeric(38, 0) PRIMARY KEY,
    block_timestamp           numeric(20, 0) NOT NULL,
    block_height              numeric(20, 0) NOT NULL,
    receipt_id                text,
    transaction_hash          text,
    affected_account_id       text           NOT NULL,
    involved_account_id       text,
    direction                 text           NOT NULL,
    cause                     text           NOT NULL,
    status                    text           NOT NULL,
    delta_nonstaked_amount    numeric(40, 0) NOT NULL,
    absolute_nonstaked_amount numeric(40, 0) NOT NULL,
    delta_staked_amount       numeric(40, 0) NOT NULL,
    absolute_staked_amount    numeric(40, 0) NOT NULL
);

CREATE INDEX CONCURRENTLY near_balance_events_block_height_idx ON near_balance_events (block_height);
CREATE INDEX CONCURRENTLY near_balance_events_affected_account_idx ON near_balance_events (affected_account_id);
CREATE INDEX CONCURRENTLY near_balance_events_receipt_id_idx ON near_balance_events (receipt_id);
CREATE INDEX CONCURRENTLY near_balance_events_tx_hash_idx ON near_balance_events (transaction_hash);

-- ALTER TABLE near_balance_events
--     ADD CONSTRAINT near_balance_events_receipt_id_fk FOREIGN KEY (receipt_id) REFERENCES action_receipts(receipt_id);
-- ALTER TABLE near_balance_events
--     ADD CONSTRAINT near_balance_events_tx_hash_fk FOREIGN KEY (transaction_hash) REFERENCES transactions(transaction_hash);
