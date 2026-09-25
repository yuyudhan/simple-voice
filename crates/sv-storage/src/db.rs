// FilePath: crates/sv-storage/src/db.rs
//! Opening, upgrading and moving the database.

use std::ffi::OsString;
use std::fs::{self, DirBuilder, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use sv_domain::{AppError, AppResult};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::paths;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

const KEPT_BACKUPS: usize = 3;
const BACKUP_PREFIX: &str = "simple-voice-";
const MAX_CONNECTIONS: u32 = 4;

/// Handle to the database. Cheap to clone; every clone sees a relocation.
///
/// Queries hold the read lock for their duration and `relocate` takes the write lock, so no
/// write can land in the old file while it is being copied.
#[derive(Clone, Debug)]
pub struct Db {
    inner: Arc<RwLock<Inner>>,
}

#[derive(Debug)]
pub(crate) struct Inner {
    pub(crate) pool: SqlitePool,
    pub(crate) db_path: PathBuf,
    /// Directory holding `location` and `backups/` (normally `~/.simplevoice`).
    pub(crate) data_dir: PathBuf,
}

impl Inner {
    pub(crate) fn database_dir(&self) -> &Path {
        self.db_path.parent().unwrap_or(self.data_dir.as_path())
    }
}

impl Db {
    /// Opens the user's database: creates `~/.simplevoice`, follows the `location` pointer, backs
    /// up an existing database that has pending migrations, then migrates.
    pub async fn open() -> AppResult<Db> {
        let data_dir = paths::data_dir()?;
        let db_dir = paths::database_dir_in(&data_dir)?;
        if !db_dir.is_dir() {
            // Creating a fresh database here would silently hide the user's history the moment
            // the drive holding it comes back.
            return Err(AppError::Database(format!(
                "The database folder {} is not available. Reconnect the drive that holds it, or \
                 delete {} to start over with a new database.",
                db_dir.display(),
                data_dir.join(paths::LOCATION_FILE).display()
            )));
        }
        Self::open_with(data_dir, db_dir, &MIGRATOR).await
    }

    /// Opens (or creates) a database in `dir`, which also serves as the data directory for the
    /// `location` pointer and `backups/`.
    pub async fn open_at(dir: &Path) -> AppResult<Db> {
        paths::ensure_private_dir(dir)?;
        Self::open_with(dir.to_path_buf(), dir.to_path_buf(), &MIGRATOR).await
    }

    async fn open_with(data_dir: PathBuf, db_dir: PathBuf, migrator: &Migrator) -> AppResult<Db> {
        let db_path = db_dir.join(paths::DATABASE_FILE);
        let backups = paths::backups_dir_in(&data_dir)?;
        let pool = open_pool(&db_path, &backups, migrator).await?;
        Ok(Db {
            inner: Arc::new(RwLock::new(Inner {
                pool,
                db_path,
                data_dir,
            })),
        })
    }

    pub(crate) async fn read(&self) -> RwLockReadGuard<'_, Inner> {
        self.inner.read().await
    }

    pub async fn database_path(&self) -> PathBuf {
        self.inner.read().await.db_path.clone()
    }

    /// Moves the database into `new_dir` and remembers the new location across restarts.
    pub async fn relocate(&self, new_dir: &Path) -> AppResult<()> {
        let mut inner = self.inner.write().await;
        DirBuilder::new().recursive(true).create(new_dir)?;
        let new_dir = fs::canonicalize(new_dir)?;
        if paths::same_dir(&new_dir, inner.database_dir()) {
            return Ok(());
        }
        let new_path = new_dir.join(paths::DATABASE_FILE);
        if new_path.exists() {
            return Err(AppError::InvalidInput(format!(
                "{} already contains a Simple Voice database. Choose another folder.",
                new_dir.display()
            )));
        }

        vacuum_into(&inner.pool, &new_path).await?;
        let backups = paths::backups_dir_in(&inner.data_dir)?;
        let opened = match paths::set_private_file(&new_path) {
            Ok(()) => open_pool(&new_path, &backups, &MIGRATOR).await,
            Err(error) => Err(error),
        };
        let pool = match opened {
            Ok(pool) => pool,
            Err(error) => {
                discard_copy(&new_path);
                return Err(error);
            }
        };
        if let Err(error) = paths::write_location(&inner.data_dir, Some(new_dir.as_path())) {
            pool.close().await;
            discard_copy(&new_path);
            return Err(error);
        }

        let old_pool = std::mem::replace(&mut inner.pool, pool);
        let old_path = std::mem::replace(&mut inner.db_path, new_path);
        old_pool.close().await;
        // The copy is live and the pointer written; a leftover old file only wastes space.
        if let Err(error) = remove_database_files(&old_path) {
            tracing::warn!(path = %old_path.display(), %error, "could not remove old database");
        }
        Ok(())
    }
}

async fn open_pool(
    db_path: &Path,
    backups_dir: &Path,
    migrator: &Migrator,
) -> AppResult<SqlitePool> {
    let existed = fs::metadata(db_path).is_ok_and(|meta| meta.len() > 0);
    // Create the file ourselves so it is never world-readable, not even briefly.
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(db_path)?;
    paths::set_private_file(db_path)?;

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .connect_with(options)
        .await
        .map_err(AppError::database)?;

    match upgrade(&pool, existed, backups_dir, migrator).await {
        Ok(()) => Ok(pool),
        Err(error) => {
            pool.close().await;
            Err(error)
        }
    }
}

async fn upgrade(
    pool: &SqlitePool,
    existed: bool,
    backups_dir: &Path,
    migrator: &Migrator,
) -> AppResult<()> {
    let applied = applied_versions(pool).await?;
    if let Some(newest) = applied
        .iter()
        .copied()
        .filter(|v| !migrator.version_exists(*v))
        .max()
    {
        return Err(AppError::Database(format!(
            "This database was created by a newer version of Simple Voice (schema version \
             {newest}). Update Simple Voice to open it; the file was left untouched."
        )));
    }
    let pending = migrator
        .iter()
        .any(|migration| !applied.contains(&migration.version));
    if !pending {
        return Ok(());
    }
    if existed {
        let current = applied.iter().copied().max().unwrap_or(0);
        backup(pool, backups_dir, current).await?;
    }
    migrator.run(pool).await.map_err(AppError::database)
}

async fn applied_versions(pool: &SqlitePool) -> AppResult<Vec<i64>> {
    let tables = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!: i64" FROM sqlite_master
           WHERE type = 'table' AND name = '_sqlx_migrations'"#
    )
    .fetch_one(pool)
    .await
    .map_err(AppError::database)?;
    if tables == 0 {
        return Ok(Vec::new());
    }
    sqlx::query_scalar!(r#"SELECT version AS "version!: i64" FROM _sqlx_migrations"#)
        .fetch_all(pool)
        .await
        .map_err(AppError::database)
}

async fn backup(pool: &SqlitePool, backups_dir: &Path, version: i64) -> AppResult<()> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let target = backups_dir.join(format!("{BACKUP_PREFIX}{stamp}-v{version}.db"));
    // VACUUM INTO rather than a file copy: with WAL, committed pages may still live only in the
    // -wal file, and this also yields a compact, consistent snapshot.
    vacuum_into(pool, &target).await?;
    paths::set_private_file(&target)?;
    tracing::info!(path = %target.display(), "backed up database before migrating");
    prune_backups(backups_dir)
}

fn prune_backups(backups_dir: &Path) -> AppResult<()> {
    let mut backups: Vec<PathBuf> = fs::read_dir(backups_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(BACKUP_PREFIX) && name.ends_with(".db"))
        })
        .collect();
    // Names start with a fixed-width timestamp, so lexical order is chronological.
    backups.sort();
    let excess = backups.len().saturating_sub(KEPT_BACKUPS);
    for old in backups.iter().take(excess) {
        paths::remove_if_exists(old)?;
    }
    Ok(())
}

pub(crate) async fn vacuum_into(pool: &SqlitePool, target: &Path) -> AppResult<()> {
    let target = target.to_str().ok_or_else(|| {
        AppError::InvalidInput("The database folder path must be valid UTF-8".to_owned())
    })?;
    sqlx::query!("VACUUM INTO ?1", target)
        .execute(pool)
        .await
        .map_err(AppError::database)?;
    Ok(())
}

fn remove_database_files(db_path: &Path) -> AppResult<()> {
    paths::remove_if_exists(db_path)?;
    for suffix in ["-wal", "-shm"] {
        let mut name = OsString::from(db_path.as_os_str());
        name.push(suffix);
        paths::remove_if_exists(Path::new(&name))?;
    }
    Ok(())
}

fn discard_copy(path: &Path) {
    if let Err(error) = remove_database_files(path) {
        tracing::warn!(path = %path.display(), %error, "could not remove partial database copy");
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::os::unix::fs::PermissionsExt;

    use sqlx::migrate::{Migration, MigrationType};
    use sqlx::SqlStr;

    use super::*;

    fn backups_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.join("backups"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn migrator_with_extra_version() -> Migrator {
        let mut migrations: Vec<Migration> = MIGRATOR.iter().cloned().collect();
        migrations.push(Migration::new(
            9_999,
            Cow::Borrowed("future"),
            MigrationType::Simple,
            SqlStr::from_static("CREATE TABLE future_table (id INTEGER PRIMARY KEY NOT NULL);"),
            false,
        ));
        Migrator::with_migrations(migrations)
    }

    async fn table_count(db: &Db) -> i64 {
        let inner = db.read().await;
        sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "n!: i64" FROM sqlite_master
               WHERE type = 'table' AND name IN ('settings', 'dictionary', 'history')"#
        )
        .fetch_one(&inner.pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn fresh_database_gets_schema_without_backup() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        assert_eq!(table_count(&db).await, 3);
        assert!(backups_in(dir.path()).is_empty());
        let mode = fs::metadata(db.database_path().await)
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[tokio::test]
    async fn reopening_is_idempotent_and_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open_at(dir.path()).await.unwrap();
        db.add_dictionary_entry("ArgoCD".to_owned(), None)
            .await
            .unwrap();
        drop(db);

        let reopened = Db::open_at(dir.path()).await.unwrap();
        assert_eq!(reopened.dictionary().await.unwrap().len(), 1);
        assert!(backups_in(dir.path()).is_empty());
    }

    #[tokio::test]
    async fn pending_migration_on_existing_database_takes_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        drop(Db::open_at(dir.path()).await.unwrap());

        let newer = migrator_with_extra_version();
        let upgraded =
            Db::open_with(dir.path().to_path_buf(), dir.path().to_path_buf(), &newer).await;
        assert!(upgraded.is_ok());
        let backups = backups_in(dir.path());
        assert_eq!(backups.len(), 1);
        assert!(backups[0].starts_with(BACKUP_PREFIX) && backups[0].ends_with("-v1.db"));

        // Nothing pending on the next launch of the same version: no second backup.
        drop(upgraded);
        Db::open_with(dir.path().to_path_buf(), dir.path().to_path_buf(), &newer)
            .await
            .unwrap();
        assert_eq!(backups_in(dir.path()).len(), 1);
    }

    #[tokio::test]
    async fn database_from_a_newer_app_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let newer = migrator_with_extra_version();
        Db::open_with(dir.path().to_path_buf(), dir.path().to_path_buf(), &newer)
            .await
            .unwrap();

        let error = Db::open_at(dir.path()).await.unwrap_err();
        assert!(
            error.to_string().contains("newer version of Simple Voice"),
            "{error}"
        );
        assert!(backups_in(dir.path()).is_empty());
    }

    #[test]
    fn only_the_newest_backups_are_kept() {
        let dir = tempfile::tempdir().unwrap();
        let backups = paths::backups_dir_in(dir.path()).unwrap();
        for stamp in [
            "20260101-000000",
            "20260102-000000",
            "20260103-000000",
            "20260104-000000",
        ] {
            fs::write(backups.join(format!("{BACKUP_PREFIX}{stamp}-v1.db")), b"x").unwrap();
        }
        prune_backups(&backups).unwrap();
        assert_eq!(
            backups_in(dir.path()),
            vec![
                "simple-voice-20260102-000000-v1.db",
                "simple-voice-20260103-000000-v1.db",
                "simple-voice-20260104-000000-v1.db",
            ]
        );
    }

    #[tokio::test]
    async fn relocate_moves_the_database_and_remembers_it() {
        let data = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let db = Db::open_at(data.path()).await.unwrap();
        db.add_dictionary_entry("Ghostty".to_owned(), None)
            .await
            .unwrap();
        let old_path = db.database_path().await;

        db.relocate(target.path()).await.unwrap();
        let new_path = db.database_path().await;
        assert_eq!(
            new_path,
            fs::canonicalize(target.path())
                .unwrap()
                .join(paths::DATABASE_FILE)
        );
        assert!(!old_path.exists());
        assert_eq!(
            paths::database_dir_in(data.path()).unwrap(),
            fs::canonicalize(target.path()).unwrap()
        );
        assert_eq!(db.dictionary().await.unwrap()[0].phrase, "Ghostty");
        let settings = db.settings().await.unwrap();
        assert_eq!(
            PathBuf::from(settings.database_dir),
            fs::canonicalize(target.path()).unwrap()
        );

        // Moving back to the data directory drops the pointer.
        db.relocate(data.path()).await.unwrap();
        assert_eq!(paths::database_dir_in(data.path()).unwrap(), data.path());
        assert_eq!(db.dictionary().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn relocate_refuses_a_folder_with_a_database() {
        let data = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        fs::write(target.path().join(paths::DATABASE_FILE), b"").unwrap();
        let db = Db::open_at(data.path()).await.unwrap();
        let error = db.relocate(target.path()).await.unwrap_err();
        assert!(matches!(error, AppError::InvalidInput(_)));
        assert_eq!(
            db.database_path().await,
            data.path().join(paths::DATABASE_FILE)
        );
    }
}
