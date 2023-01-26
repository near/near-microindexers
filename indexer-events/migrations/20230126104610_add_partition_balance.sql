-- Check current MIN and MAX for the range partition key 
SELECT min(event_index), max(event_index) FROM near_balance_events;
-- Current result is:
-- min: 15953682107627827960000000000000000,              max 16747588426916425110000000030000024 
--      15953682100000000000000000000000000 (Jul 21 2020)     16747588420000000000000000000000000 (Jan 26, 2023)
-- This table has 644 GB, this column has epoch timestamp as prefix

BEGIN TRANSACTION;
-- Rename table and indexes to <table/index>_old
ALTER TABLE near_balance_events RENAME TO near_balance_events_old;

ALTER TABLE near_balance_events_affected_account_idx RENAME TO near_balance_events_affected_account_idx_old;
ALTER TABLE near_balance_events_block_height_idx RENAME TO near_balance_events_block_height_idx_old;
ALTER TABLE near_balance_events_receipt_id_idx RENAME TO near_balance_events_receipt_id_idx_old;
ALTER TABLE near_balance_events_tx_hash_idx RENAME TO near_balance_events_tx_hash_idx_old;
ALTER TABLE near_balance_events_pkey RENAME TO near_balance_events_pkey_old;

-- Re-Create the table with partition config
CREATE TABLE public.near_balance_events (
	event_index numeric(38) NOT NULL,
	block_timestamp numeric(20) NOT NULL,
	block_height numeric(20) NOT NULL,
	receipt_id text NULL,
	transaction_hash text NULL,
	affected_account_id text NOT NULL,
	involved_account_id text NULL,
	direction text NOT NULL,
	cause text NOT NULL,
	status text NOT NULL,
	delta_nonstaked_amount numeric(40) NOT NULL,
	absolute_nonstaked_amount numeric(40) NOT NULL,
	delta_staked_amount numeric(40) NOT NULL,
	absolute_staked_amount numeric(40) NOT NULL,
	CONSTRAINT near_balance_events_pkey PRIMARY KEY (event_index)
)
PARTITION BY RANGE (event_index);

-- Re-Create indexes for the new table
CREATE INDEX near_balance_events_affected_accountx ON public.near_balance_events USING btree (affected_account_id);
CREATE INDEX near_balance_events_block_height_idx ON public.near_balance_events USING btree (block_height);
CREATE INDEX near_balance_events_receipt_id_idx ON public.near_balance_events USING btree (receipt_id);
CREATE INDEX near_balance_events_tx_hash_idx ON public.near_balance_events USING btree (transaction_hash);

-- Create a partition for the first days of Feb/2023 to transition from month to week of the month range
CREATE TABLE near_balance_events_p202302_month2week PARTITION OF near_balance_events FOR VALUES FROM (fn_timestamp2nanosec(TIMESTAMP '2023-02-01')) TO (fn_timestamp2nanosec(TIMESTAMP '2023-02-06'));

COMMIT;

-- Create a check constraint in the old table (2020-07-01 to 2023-02-01)
ALTER TABLE near_balance_events_old ADD CONSTRAINT near_balance_events_old_check_constraint CHECK (event_index >= fn_timestamp2nanosec(TIMESTAMP '2020-07-01') AND event_index < fn_timestamp2nanosec(TIMESTAMP '2023-02-01')) NOT VALID;

-- Attach the old table as partition with previous data (2020-07-01 to 2023-02-01)
ALTER TABLE near_balance_events ATTACH PARTITION near_balance_events_old FOR VALUES FROM (fn_timestamp2nanosec(TIMESTAMP '2020-07-01')) TO (fn_timestamp2nanosec(TIMESTAMP '2023-02-01'));

-- Now we can drop the old table constraint 
ALTER TABLE near_balance_events_old DROP CONSTRAINT near_balance_events_old_check_constraint;
-- DONE!!! At this point the table is usable again :)

SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202301', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2023-01-01'), fn_timestamp2nanosec(TIMESTAMP '2023-02-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202212', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-12-01'), fn_timestamp2nanosec(TIMESTAMP '2023-01-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202211', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-11-01'), fn_timestamp2nanosec(TIMESTAMP '2022-12-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202210', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-10-01'), fn_timestamp2nanosec(TIMESTAMP '2022-11-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202209', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-09-01'), fn_timestamp2nanosec(TIMESTAMP '2022-10-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202208', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-08-01'), fn_timestamp2nanosec(TIMESTAMP '2022-09-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202207', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-07-01'), fn_timestamp2nanosec(TIMESTAMP '2022-08-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202201_to_06', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-01-01'), fn_timestamp2nanosec(TIMESTAMP '2022-07-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202107_to_12', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2021-07-01'), fn_timestamp2nanosec(TIMESTAMP '2022-01-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202101_to_06', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2021-01-01'), fn_timestamp2nanosec(TIMESTAMP '2021-07-01'));
SELECT fn_partition_by_range('near_balance_events', 'near_balance_events_old', 'near_balance_events_p202007_to_12', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2020-07-01'), fn_timestamp2nanosec(TIMESTAMP '2021-01-01'));

-- After all the data moved into the correspondent partition we can now detach and drop the old partition without the need to execute VACUUM
-- NOTE: We could use VACUUM FULL to recover OS disk space but it would require exclusive lock
ALTER TABLE near_balance_events DETACH PARTITION near_balance_events_old;
DROP TABLE near_balance_events_old;

-- To automatically create new partitions. 
--  Every Monday at 11am creates a new partition for the next week
SELECT cron.schedule('0 11 * * 1', $$SELECT fn_create_next_partition('near_balance_events', CURRENT_DATE, 'week', 'yyyyMM_W')$$);
