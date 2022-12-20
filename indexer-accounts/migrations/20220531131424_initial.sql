CREATE TABLE accounts
(
    account_id               text           NOT NULL,
    created_by_receipt_id    text,
    deleted_by_receipt_id    text,
    created_by_block_height numeric(20, 0) NOT NULL,
    deleted_by_block_height numeric(20, 0),
    PRIMARY KEY (account_id, created_by_block_height)
);
ALTER TABLE accounts
    ADD CONSTRAINT accounts_created_by_receipt_id_fk FOREIGN KEY (created_by_receipt_id) REFERENCES action_receipts(receipt_id);
ALTER TABLE accounts
    ADD CONSTRAINT accounts_deleted_by_receipt_id_fk FOREIGN KEY (deleted_by_receipt_id) REFERENCES action_receipts(receipt_id);
CREATE INDEX CONCURRENTLY accounts_account_id_idx ON accounts (account_id);
CREATE INDEX CONCURRENTLY accounts_created_by_block_height_idx ON accounts (created_by_block_height);
CREATE INDEX CONCURRENTLY accounts_deleted_by_block_height_idx ON accounts (deleted_by_block_height);

CREATE TABLE access_keys
(
    public_key               text           NOT NULL,
    account_id               text           NOT NULL,
    created_by_receipt_id    text,
    deleted_by_receipt_id    text,
    created_by_block_height numeric(20, 0) NOT NULL,
    deleted_by_block_height numeric(20, 0),
    permission_kind          text           NOT NULL,
    PRIMARY KEY (public_key, account_id)
);
ALTER TABLE access_keys
    ADD CONSTRAINT access_keys_created_by_receipt_id_fk FOREIGN KEY (created_by_receipt_id) REFERENCES action_receipts(receipt_id);
ALTER TABLE access_keys
    ADD CONSTRAINT access_keys_deleted_by_receipt_id_fk FOREIGN KEY (deleted_by_receipt_id) REFERENCES action_receipts(receipt_id);
CREATE INDEX CONCURRENTLY access_keys_account_id_idx ON access_keys (account_id);
CREATE INDEX CONCURRENTLY access_keys_public_key_idx ON access_keys (public_key);
CREATE INDEX CONCURRENTLY access_keys_created_by_block_height_idx ON access_keys (created_by_block_height);
CREATE INDEX CONCURRENTLY access_keys_deleted_by_block_height_idx ON access_keys (deleted_by_block_height);
