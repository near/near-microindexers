-- Check current MIN and MAX for the range partition key 
SELECT min(event_index), max(event_index) FROM coin_events;
-- Current result is:
-- min: 16136043940348625390000000008000000,              max 16742378612931940910000003001000005 
--      16136043940000000000000000000000000 (Feb 17 2021)     16742378610000000000000000000000000 (Jan 20, 2023)
-- This table has 329 GB, this columns has epoch timestamp as prefix
-- We have almost 2 years of data, we can try to split the data in partitions by months
-- NOTE: Team suggested to use a new table name fungible_token_events instead of coin_events

-- Support function to convert timestamo to numeric 38 nanosec
CREATE OR REPLACE FUNCTION fn_timestamp2nanosec(_ptimestamp TIMESTAMP)
  RETURNS numeric(38, 0)
  LANGUAGE plpgsql AS
$func$
BEGIN
	RETURN (EXTRACT(EPOCH FROM (_ptimestamp))::numeric(38, 0) * pow(10,25)::numeric(38, 0));
END
$func$;

BEGIN TRANSACTION;
-- Rename table and indexes to <table/index>_old
ALTER TABLE coin_events RENAME TO fungible_token_events_old;

ALTER TABLE coin_events_affected_account_id_idx RENAME TO fungible_token_events_affected_account_id_idx_old;
ALTER TABLE coin_events_block_height_idx RENAME TO fungible_token_events_block_height_idx_old;
ALTER TABLE coin_events_receipt_id_idx RENAME TO fungible_token_events_receipt_id_idx_old;

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
CREATE INDEX fungible_token_events_affected_account_id_idx ON public.fungible_token_events USING btree (affected_account_id);
CREATE INDEX fungible_token_events_contract_account_id_idx ON public.fungible_token_events USING btree (contract_account_id);
-- Create a partition to start after next month (2023-02-01 to 2023-03-01)
CREATE TABLE fungible_token_events_p202302 PARTITION OF fungible_token_events FOR VALUES FROM (fn_timestamp2nanosec(TIMESTAMP '2023-02-01')) TO (fn_timestamp2nanosec(TIMESTAMP '2023-03-01'));

COMMIT;

-- Create a check constraint in the old table (2021-01-01 to 2023-02-01)
ALTER TABLE fungible_token_events_old ADD CONSTRAINT fungible_token_events_old_check_constraint CHECK (event_index >= fn_timestamp2nanosec(TIMESTAMP '2021-01-01') AND event_index < fn_timestamp2nanosec(TIMESTAMP '2023-02-01')) NOT VALID;

-- Attach the old table as partition with previous data (2021-01-01 to 2023-02-01)
ALTER TABLE fungible_token_events ATTACH PARTITION fungible_token_events_old FOR VALUES FROM (fn_timestamp2nanosec(TIMESTAMP '2021-01-01')) TO (fn_timestamp2nanosec(TIMESTAMP '2023-02-01'));

-- Now we can drop the old table constraint 
ALTER TABLE fungible_token_events_old DROP CONSTRAINT fungible_token_events_old_check_constraint;
-- DONE!!! At this point the table is usable again :)


CREATE OR REPLACE FUNCTION fn_partition_by_range(_tbl text, _poldname text, _pnewname text, _pkey text, _start numeric, _end numeric)
  RETURNS void
  LANGUAGE plpgsql AS
$func$
BEGIN
	-- Detach old partition
	EXECUTE 'ALTER TABLE ' || _tbl || ' DETACH PARTITION ' || _poldname;
	-- Create partition from _start to _end
	EXECUTE 'CREATE TABLE ' || _pnewname || ' PARTITION OF ' || _tbl || ' FOR VALUES FROM (' || _start || ') TO (' || _end || ')';
	-- Insert data to partition from _start to _end
	EXECUTE 'INSERT INTO ' || _pnewname || ' SELECT * FROM ' || _poldname || ' WHERE ' || _pkey || ' >= ' || _start || ' AND ' || _pkey || ' < ' || _end;
	-- Delete data from old partition from _start to _end
	EXECUTE 'DELETE FROM ' || _poldname || ' WHERE ' || _pkey || ' >= ' || _start || ' AND ' || _pkey || ' < ' || _end;
	-- Add check on old partition from 0 to _start
	EXECUTE 'ALTER TABLE ' || _poldname || ' ADD CONSTRAINT ' || _poldname || '_check_constraint CHECK (' || _pkey || ' >= 0 AND ' || _pkey || ' < ' || _start || ') NOT VALID';
	-- Re-Attach old partition from 0 to _start
	EXECUTE 'ALTER TABLE ' || _tbl || ' ATTACH PARTITION ' || _poldname || ' FOR VALUES FROM (0) TO (' || _start || ')';
	-- Drop the check on old partition
	EXECUTE 'ALTER TABLE ' || _poldname || ' DROP CONSTRAINT ' || _poldname || '_check_constraint';
END
$func$;

SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202301', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2023-01-01'), fn_timestamp2nanosec(TIMESTAMP '2023-02-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202212', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-12-01'), fn_timestamp2nanosec(TIMESTAMP '2023-01-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202211', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-11-01'), fn_timestamp2nanosec(TIMESTAMP '2022-12-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202210', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-10-01'), fn_timestamp2nanosec(TIMESTAMP '2022-11-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202209', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-09-01'), fn_timestamp2nanosec(TIMESTAMP '2022-10-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202208', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-08-01'), fn_timestamp2nanosec(TIMESTAMP '2022-09-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202207', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-07-01'), fn_timestamp2nanosec(TIMESTAMP '2022-08-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202201_to_06', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2022-01-01'), fn_timestamp2nanosec(TIMESTAMP '2022-07-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202107_to_12', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2021-07-01'), fn_timestamp2nanosec(TIMESTAMP '2022-01-01'));
SELECT fn_partition_by_range('fungible_token_events', 'fungible_token_events_old', 'fungible_token_events_p202101_to_06', 'event_index', fn_timestamp2nanosec(TIMESTAMP '2021-01-01'), fn_timestamp2nanosec(TIMESTAMP '2021-07-01'));

-- After all the data moved into the correspondent partition we can now detach and drop the old partition without the need to execute VACUUM
-- NOTE: We could use VACUUM FULL to recover OS disk space but it would require exclusive lock
ALTER TABLE fungible_token_events DETACH PARTITION fungible_token_events_old;
DROP TABLE fungible_token_events_old;

-- To automatically create new partitions. Examples:
-- select fn_create_next_partition('mytable', current_date, 'month', 'yyyyMM');
-- select fn_create_next_partition('mytable', current_date, 'day', 'yyyyMMDD');
-- select fn_create_next_partition('mytable', current_date, 'week', 'yyyyMMW');
CREATE OR REPLACE FUNCTION fn_create_next_partition(_tbl text, _timestamp date, _interval text, _postfixformat text)
  RETURNS void
  LANGUAGE plpgsql AS
$func$
DECLARE
	_start numeric;
	_end numeric; 
	_partition_name text;
begin
	_start := fn_timestamp2nanosec(date_trunc(_interval, _timestamp + ('1 ' || _interval)::interval));
	_end := fn_timestamp2nanosec(date_trunc(_interval, _timestamp + ('2 ' || _interval)::interval));
	_partition_name := TO_CHAR(date_trunc(_interval, _timestamp + ('1 ' || _interval)::interval), _postfixformat);
	-- Create partition from _start to _end
	EXECUTE 'CREATE TABLE IF NOT EXISTS ' || _tbl || '_p' || _partition_name || ' PARTITION OF ' || _tbl || ' FOR VALUES FROM (' || _start || ') TO (' || _end || ')';
END
$func$;

-- GCP Cloud SQL requires these flags:
-- cloudsql.enable_pg_cron = On
-- cron.database_name = indexer_balances_mainnet

create extension pg_cron;

-- Moving forward we can partition the table by week
--  Every Monday at 10am creates a new partition for the next week
SELECT cron.schedule('0 10 * * 1', $$SELECT fn_create_next_partition('fungible_token_events', CURRENT_DATE, 'week', 'yyyyMM_W')$$);
-- Create a partition for the first days of Feb/2023 to transition from month to week of the month range
CREATE TABLE fungible_token_events_p202302_month2week PARTITION OF fungible_token_events FOR VALUES FROM (fn_timestamp2nanosec(TIMESTAMP '2023-02-01')) TO (fn_timestamp2nanosec(TIMESTAMP '2023-02-06'));
