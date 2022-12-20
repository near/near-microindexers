CREATE TABLE nft_events
(
    event_index           numeric(38, 0) PRIMARY KEY,
    standard              text           NOT NULL,
    receipt_id            text           NOT NULL,
    block_height          numeric(20, 0) NOT NULL,
    block_timestamp       numeric(20, 0) NOT NULL,
    -- account_id of the contract itself. In a simple words, it's the owner/creator of NFT contract
    contract_account_id   text           NOT NULL,
    -- Unique ID of the token
    token_id              text           NOT NULL,
    cause                 text           NOT NULL,
    status                text           NOT NULL,
    -- Previous owner of the token. Null if we have nft_event_kind 'MINT'.
    old_owner_account_id  text,
    -- New owner of the token. Null if we have nft_event_kind 'BURN'.
    new_owner_account_id  text,
    authorized_account_id text,
    event_memo            text
);

CREATE INDEX CONCURRENTLY nft_events_block_height_idx ON nft_events (block_height);
CREATE INDEX CONCURRENTLY nft_events_receipt_id_idx ON nft_events (receipt_id);
CREATE INDEX CONCURRENTLY nft_events_old_owner_account_id_idx ON nft_events (old_owner_account_id);
CREATE INDEX CONCURRENTLY nft_events_new_owner_account_id_idx ON nft_events (new_owner_account_id);

-- ALTER TABLE nft_events
--     ADD CONSTRAINT nft_events_fk
--         FOREIGN KEY (receipt_id) REFERENCES receipts (receipt_id) ON DELETE CASCADE;

CREATE TABLE coin_events
(
    event_index         numeric(38, 0) PRIMARY KEY,
    standard            text           NOT NULL,
    receipt_id          text           NOT NULL,
    block_height        numeric(20, 0) NOT NULL,
    block_timestamp     numeric(20, 0) NOT NULL,
    -- account_id of the contract itself. In a simple words, it's the owner/creator of FT contract
    contract_account_id text           NOT NULL,
    affected_account_id text           NOT NULL,
    involved_account_id text,
    delta_amount        numeric(40, 0) NOT NULL,
    cause               text           NOT NULL,
    status              text           NOT NULL,
    -- Optional message associated with token movement.
    event_memo          text
);

CREATE INDEX CONCURRENTLY coin_events_block_height_idx ON coin_events (block_height);
CREATE INDEX CONCURRENTLY coin_events_receipt_id_idx ON coin_events (receipt_id);
CREATE INDEX CONCURRENTLY coin_events_affected_account_id_idx ON coin_events (affected_account_id);

-- ALTER TABLE coin_events
--     ADD CONSTRAINT coin_events_fk
--         FOREIGN KEY (receipt_id) REFERENCES receipts (receipt_id) ON DELETE CASCADE;
