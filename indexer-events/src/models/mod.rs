use futures::future::try_join_all;
use std::fmt::Write;

pub use indexer_events::FieldCount;

pub(crate) mod coin_events;
pub(crate) mod contracts;
pub(crate) mod nft_events;

pub trait FieldCount {
    /// Get the number of fields on a struct.
    fn field_count() -> usize;
}

pub trait SqlMethods {
    fn add_to_args(&self, args: &mut sqlx::postgres::PgArguments);
    fn insert_query(count: usize) -> anyhow::Result<String>;
    fn name() -> String;
}

pub async fn chunked_insert<T: SqlMethods + std::fmt::Debug>(
    pool: &sqlx::Pool<sqlx::Postgres>,
    items: &[T],
) -> anyhow::Result<()> {
    let futures = items
        .chunks(crate::db_adapters::CHUNK_SIZE_FOR_BATCH_INSERT)
        .map(|items_part| insert_retry_or_panic(pool, items_part, crate::db_adapters::RETRY_COUNT));
    try_join_all(futures).await.map(|_| ())
}

async fn insert_retry_or_panic<T: SqlMethods + std::fmt::Debug>(
    pool: &sqlx::Pool<sqlx::Postgres>,
    items: &[T],
    retry_count: usize,
) -> anyhow::Result<()> {
    let mut interval = crate::INTERVAL;
    let mut retry_attempt = 0usize;
    let query = T::insert_query(items.len())?;

    loop {
        if retry_attempt == retry_count {
            return Err(anyhow::anyhow!(
                "Failed to perform query to database after {} attempts. Stop trying.",
                retry_count
            ));
        }
        retry_attempt += 1;

        let mut args = sqlx::postgres::PgArguments::default();
        for item in items {
            item.add_to_args(&mut args);
        }

        match sqlx::query_with(&query, args).execute(pool).await {
            Ok(_) => break,
            Err(async_error) => {
                tracing::warn!(
                    target: crate::LOGGING_PREFIX,
                    "Error occurred during {}:\n{} were not stored. \n{:#?} \n Retrying in {} milliseconds...",
                    async_error,
                    &T::name(),
                    &items,
                    interval.as_millis(),
                );
                tokio::time::sleep(interval).await;
                if interval < crate::MAX_DELAY_TIME {
                    interval *= 2;
                }
            }
        }
    }
    Ok(())
}

// Generates `($1, $2), ($3, $4)`
pub(crate) fn create_placeholders(
    mut items_count: usize,
    fields_count: usize,
) -> anyhow::Result<String> {
    if items_count < 1 {
        return Err(anyhow::anyhow!("At least 1 item expected"));
    }

    let mut start_num: usize = 1;
    let mut res = create_placeholder(&mut start_num, fields_count)?;
    items_count -= 1;
    while items_count > 0 {
        write!(
            res,
            ", {}",
            create_placeholder(&mut start_num, fields_count)?
        )?;
        items_count -= 1;
    }

    Ok(res)
}

// Generates `($1, $2, $3)`
pub(crate) fn create_placeholder(
    start_num: &mut usize,
    mut fields_count: usize,
) -> anyhow::Result<String> {
    if fields_count < 1 {
        return Err(anyhow::anyhow!("At least 1 field expected"));
    }
    let mut item = format!("(${}", start_num);
    *start_num += 1;
    fields_count -= 1;
    while fields_count > 0 {
        write!(item, ", ${}", start_num)?;
        *start_num += 1;
        fields_count -= 1;
    }
    item += ")";
    Ok(item)
}
