CREATE TABLE meta (
    indexer_id                  VARCHAR(50)     PRIMARY KEY,
    last_processed_block_height numeric(20, 0)  NOT NULL
)
