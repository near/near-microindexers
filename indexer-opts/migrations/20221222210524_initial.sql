CREATE TABLE __meta
(
    indexer_id                  text PRIMARY KEY,
    indexer_type                text           NOT NULL,
    indexer_started_at          timestamptz    NOT NULL,
    last_processed_block_height numeric(20, 0) NOT NULL,
    start_block_height          numeric(20, 0) NOT NULL,
    end_block_height            numeric(20, 0)
);
