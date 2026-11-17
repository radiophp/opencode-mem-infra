//! PostgreSQL schema migrations for the Infinite Memory feature.
//!
//! `sqlx` keeps applied migration versions in one hard-coded `_sqlx_migrations`
//! table per database. The Infinite Memory migration set reuses version numbers
//! that already exist in the main set with different SQL, so running it through
//! `Migrator::run` against a database that has the main migrations applied
//! fails with `VersionMissing`/`VersionMismatch`.
//!
//! To keep long-lived databases upgradeable, Infinite Memory migrations are
//! tracked in a dedicated ledger table (`_sqlx_migrations_infinite`) and applied
//! directly. Every migration file is written to be idempotent (`IF NOT EXISTS`)
//! and the ledger insert is conflict-tolerant, so replaying a migration that
//! was applied out-of-band — or racing another instance — is safe.

use crate::StorageError;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Runs on a dedicated single-connection pool to avoid advisory lock leak.
/// See `pg_migrations.rs` module doc for rationale.
pub async fn run_infinite_memory_migrations(pool: &PgPool) -> Result<(), StorageError> {
    let migration_pool = PgPoolOptions::new()
        .max_connections(1)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .map_err(|e| StorageError::Migration(format!("Migration connection: {e}")))?;

    let result = apply_all(&migration_pool).await;

    migration_pool.close().await;

    result?;

    tracing::info!("Infinite Memory schema migrations completed");
    Ok(())
}

async fn apply_all(pool: &PgPool) -> Result<(), StorageError> {
    sqlx::raw_sql(
        r#"
        CREATE TABLE IF NOT EXISTS _sqlx_migrations_infinite (
            version BIGINT PRIMARY KEY,
            description TEXT NOT NULL,
            installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            success BOOLEAN NOT NULL
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(|e| StorageError::Migration(format!("Infinite memory migration ledger: {e}")))?;

    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations_infinite WHERE success = TRUE")
            .fetch_all(pool)
            .await
            .map_err(|e| {
                StorageError::Migration(format!("Infinite memory migration ledger read: {e}"))
            })?;

    let mut applied_now = 0usize;

    for migration in sqlx::migrate!("./migrations_infinite").iter() {
        if applied.contains(&migration.version) {
            continue;
        }

        sqlx::raw_sql(migration.sql.as_ref())
            .execute(pool)
            .await
            .map_err(|e| {
                StorageError::Migration(format!(
                    "Infinite memory migration {} ({}): {e}",
                    migration.version, migration.description
                ))
            })?;

        sqlx::query(
            "INSERT INTO _sqlx_migrations_infinite (version, description, success) \
             VALUES ($1, $2, TRUE) ON CONFLICT (version) DO NOTHING",
        )
        .bind(migration.version)
        .bind(migration.description.as_ref())
        .execute(pool)
        .await
        .map_err(|e| {
            StorageError::Migration(format!("Infinite memory migration ledger write: {e}"))
        })?;

        applied_now += 1;
    }

    if applied_now > 0 {
        tracing::info!(count = applied_now, "Infinite Memory migrations applied");
    }

    Ok(())
}
