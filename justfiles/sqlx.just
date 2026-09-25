# FilePath: justfiles/sqlx.just
# Compile-time checked SQL. Code compiles with SQLX_OFFLINE=true against the committed .sqlx/
# cache; these recipes regenerate and verify that cache against a throwaway database built from
# the real migrations. See docs/internal/development.md for the workflow.

# Recreate target/sqlx-dev.db from scratch and apply every migration.
[group('sqlx')]
sqlx-db:
    mkdir -p target
    rm -f {{sqlx_db}} {{sqlx_db}}-shm {{sqlx_db}}-wal
    DATABASE_URL={{sqlx_db_url}} sqlx database create
    DATABASE_URL={{sqlx_db_url}} sqlx migrate run --source {{migrations_dir}}

# Regenerate the committed .sqlx/ offline cache. Run after adding or changing any query macro.
[group('sqlx')]
sqlx-prepare: sqlx-db engine-present
    DATABASE_URL={{sqlx_db_url}} CARGO_TARGET_DIR=target/sqlx \
        cargo sqlx prepare --workspace -- --all-targets

# Fail when the committed .sqlx/ cache does not match the queries in the code.
[group('sqlx')]
sqlx-check: sqlx-db engine-present
    DATABASE_URL={{sqlx_db_url}} CARGO_TARGET_DIR=target/sqlx \
        cargo sqlx prepare --workspace --check -- --all-targets
