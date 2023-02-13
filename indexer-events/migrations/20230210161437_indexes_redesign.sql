-- In reality, I applied index on each partition concurrently
-- The list of partition names could be collected by
--
-- SELECT table_name
--   FROM information_schema.tables
--  WHERE table_schema='public' and table_name like 'fungible_%';
--
-- Then I generate the commands with Python script
--
-- with open("1.txt") as f:
--     for line in f:
--         line = line.strip()
--         # print("DROP INDEX " + line + "_eapi_history_idx")
--         print("CREATE INDEX CONCURRENTLY " + line + "_eapi_history_idx ON " + line + " (affected_account_id, event_index desc);")

-- Then, this command is super fast, even without CONCURRENTLY
CREATE INDEX fungible_token_events_eapi_history_idx ON fungible_token_events (affected_account_id, event_index desc);

-- These indexes are not used in production
DROP INDEX fungible_token_events_block_height_idx;
DROP INDEX fungible_token_events_receipt_id_idx;
DROP INDEX fungible_token_events_block_timestamp_idx;
-- This one could be always replaced with fungible_token_events_eapi_history_idx
DROP INDEX fungible_token_events_affected_account_id_idx;
