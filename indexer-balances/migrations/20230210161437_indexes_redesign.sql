-- In reality, I applied index on each partition concurrently
-- The list of partition names could be collected by
--
-- SELECT table_name
--   FROM information_schema.tables
--  WHERE table_schema='public' and table_name like 'near_balance_events%';
--
-- Then I generate the commands with Python script
--
-- with open("1.txt") as f:
--     for line in f:
--         line = line.strip()
--         # print("DROP INDEX " + line + "_eapi_history_idx")
--         print("CREATE INDEX CONCURRENTLY " + line + "_eapi_history_idx ON " + line + " (affected_account_id, event_index desc);")

-- Then, this command is super fast, even without CONCURRENTLY
CREATE INDEX near_balance_events_eapi_history_idx ON near_balance_events (affected_account_id, event_index desc);

-- These indexes are not used in production
DROP INDEX near_balance_events_block_height_idx;
DROP INDEX near_balance_events_receipt_id_idx;
DROP INDEX near_balance_events_tx_hash_idx;
-- This one could be always replaced with near_balance_events_eapi_history_idx
DROP INDEX near_balance_events_affected_accountx;
