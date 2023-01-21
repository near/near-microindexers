-- Adding an index by block_timestamp just in case people want to query by it
CREATE INDEX coin_events_block_timestamp_idx ON coin_events USING btree (block_timestamp);

-- Check current MIN and MAX for the range partition key 
SELECT min(event_index), max(event_index) FROM coin_events;
-- Current result is:
-- min: 16136043940348625390000000008000000,              max 16742378612931940910000003001000005 
--      16136043940000000000000000000000000 (Feb 17 2021)     16742378610000000000000000000000000 (Jan 20, 2023)
-- This table has 329 GB, this columns has epoch timestamp as prefix
-- We have almost 2 years of data, we can try to split the data in partitions of 6 months
-- 16094592000000000000000000000000000 (2021-01-01) [p1] - OK (Old)
-- 16250976000000000000000000000000000 (2021-07-01) [p2] - Later
-- 16409952000000000000000000000000000 (2022-01-01) [p3] - Later
-- 16566336000000000000000000000000000 (2022-07-01) [p4] - Later
-- 16725312000000000000000000000000000 (2023-01-01) [p5] - OK
-- 16881696000000000000000000000000000 (2023-07-01) [p6] - OK
-- 17040672000000000000000000000000000 (2024-01-01) [p7] - Later
-- NOTE: Team suggested to use a new table name fungible_token_events instead of coin_events

BEGIN TRANSACTION;
-- Rename table and indexes to <table/index>_old
ALTER TABLE coin_events RENAME TO fungible_token_events_old;

ALTER TABLE coin_events_affected_account_id_idx RENAME TO fungible_token_events_affected_account_id_idx_old;
ALTER TABLE coin_events_block_height_idx RENAME TO fungible_token_events_block_height_idx_old;
ALTER TABLE coin_events_receipt_id_idx RENAME TO fungible_token_events_receipt_id_idx_old;
ALTER TABLE coin_events_block_timestamp_idx RENAME TO fungible_token_events_block_timestamp_idx_old;

-- Re-Create the table with partition config
CREATE TABLE fungible_token_events (
	event_index numeric(38) NOT NULL,
	standard text NOT NULL,
	receipt_id text NOT NULL,
	block_height numeric(20) NOT NULL,
	block_timestamp numeric(20) NOT NULL,
	contract_account_id text NOT NULL,
	affected_account_id text NOT NULL,
	involved_account_id text NULL,
	delta_amount numeric(40) NOT NULL,
	cause text NOT NULL,
	status text NOT NULL,
	event_memo text NULL,
	CONSTRAINT fungible_token_events_pkey PRIMARY KEY (event_index)
)
PARTITION BY RANGE (event_index);

-- Re-Create indexes for the new table
CREATE INDEX fungible_token_events_affected_account_id_idx ON public.fungible_token_events USING btree (affected_account_id);
CREATE INDEX fungible_token_events_block_height_idx ON public.fungible_token_events USING btree (block_height);
CREATE INDEX fungible_token_events_receipt_id_idx ON public.fungible_token_events USING btree (receipt_id);
CREATE INDEX fungible_token_events_block_timestamp_idx ON fungible_token_events USING btree (block_timestamp);
-- Create a partition to start after the MAX found (2023-07-01 to 2024-01-01)
CREATE TABLE fungible_token_events_p202307 PARTITION OF fungible_token_events FOR VALUES FROM (16881696000000000000000000000000000) TO (17040672000000000000000000000000000);

COMMIT;

-- Create a check constraint in the old table (2021-01-01 to 2023-07-01)
ALTER TABLE fungible_token_events_old ADD CONSTRAINT fungible_token_events_old_check_constraint CHECK (event_index >= 16094592000000000000000000000000000 AND event_index < 16881696000000000000000000000000000) NOT VALID;

-- Attach the old table as partition with previous data (2021-01-01 to 2023-07-01)
ALTER TABLE fungible_token_events ATTACH PARTITION fungible_token_events_old FOR VALUES FROM (16094592000000000000000000000000000) TO (16881696000000000000000000000000000);

-- Now we can drop the old table constraint 
ALTER TABLE fungible_token_events_old DROP CONSTRAINT fungible_token_events_old_check_constraint;
-- DONE!!! At this point the table is usable again :)



-- To move data (break it down) from the large partition to smaller ones:
BEGIN TRANSACTION;
-- Detach old partition
ALTER TABLE fungible_token_events DETACH PARTITION fungible_token_events_old;
-- Create partition from 2023-01-01 to 2023-07-01
CREATE TABLE fungible_token_events_p202301 PARTITION OF fungible_token_events FOR VALUES FROM (16725312000000000000000000000000000) TO (16881696000000000000000000000000000);
-- Insert data to partition from 2023-01-01 to 2023-07-01
INSERT INTO fungible_token_events_p202301 SELECT * FROM fungible_token_events_old WHERE event_index >= 16725312000000000000000000000000000 AND event_index < 16881696000000000000000000000000000;
-- Delete data from old partition from 2023-01-01 to 2023-07-01
DELETE FROM fungible_token_events_old WHERE event_index >= 16725312000000000000000000000000000 AND event_index < 16881696000000000000000000000000000;
-- Add check on old partition from 2021-01-01 to 2023-01-01
ALTER TABLE fungible_token_events_old ADD CONSTRAINT fungible_token_events_old_check_constraint CHECK (event_index >= 16094592000000000000000000000000000 AND event_index < 16725312000000000000000000000000000) NOT VALID;
-- Re-Attach old partition from 2021-01-01 to 2023-01-01
ALTER TABLE fungible_token_events ATTACH PARTITION fungible_token_events_old FOR VALUES FROM (16094592000000000000000000000000000) TO (16725312000000000000000000000000000);
-- Drop the check on old partition
ALTER TABLE fungible_token_events_old DROP CONSTRAINT fungible_token_events_old_check_constraint;
COMMIT;

-- Vacuum the table
VACUUM fungible_token_events;