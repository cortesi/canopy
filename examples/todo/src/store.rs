//! SQLite storage for the todo example.

use std::{future::Future, io, sync::LazyLock};

use anyhow::{Result, anyhow};
use musq::{FromRow, Musq, Pool, sql, sql_as};
use tokio::runtime::{Builder, Runtime};

/// Shared timer and task driver for Musq pools used by synchronous UI
/// callbacks.
static RUNTIME: LazyLock<io::Result<Runtime>> = LazyLock::new(|| {
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_time()
        .build()
});

/// Wait for storage work, including when Canopy already runs an async executor.
fn block_on<T>(future: impl Future<Output = musq::Result<T>>) -> Result<T> {
    let runtime = RUNTIME
        .as_ref()
        .map_err(|error| anyhow!("create todo storage runtime: {error}"))?;
    let _guard = runtime.enter();
    // Pollster permits nested calls; Tokio and futures executors do not.
    Ok(pollster::block_on(future)?)
}

#[derive(Debug, Clone, FromRow)]
/// A persisted todo record.
pub struct Todo {
    /// Database identifier.
    pub id: i64,
    /// User-provided todo text.
    pub item: String,
}

#[derive(Debug, Clone)]
/// Cloneable handle to one todo database.
pub struct Store {
    /// Shared pool with one connection for this application's database.
    pool: Pool,
}

impl Store {
    /// Open or initialize a SQLite store.
    pub fn open(path: &str) -> Result<Self> {
        block_on(async {
            let options = Musq::new().max_connections(1).create_if_missing(true);
            let pool = if path == ":memory:" {
                options.open_in_memory().await?
            } else {
                options.open(path).await?
            };
            sql!(
                "CREATE TABLE IF NOT EXISTS todo (
                    id INTEGER PRIMARY KEY,
                    item TEXT NOT NULL
                );"
            )?
            .execute(&pool)
            .await?;
            Ok(Self { pool })
        })
    }

    /// Insert a todo and return its persisted record.
    pub(crate) fn add_todo(&self, item: &str) -> Result<Todo> {
        block_on(
            sql_as!("INSERT INTO todo (item) VALUES ({item}) RETURNING id, item;")?
                .fetch_one(&self.pool),
        )
    }

    /// Delete a todo by database identifier.
    pub(crate) fn delete_todo(&self, id: i64) -> Result<()> {
        block_on(sql!("DELETE FROM todo WHERE id={id};")?.execute(&self.pool))?;
        Ok(())
    }

    /// Replace all todos and return their new persisted records.
    pub(crate) fn replace_todos<'a>(
        &self,
        items: impl IntoIterator<Item = &'a str>,
    ) -> Result<Vec<Todo>> {
        block_on(async {
            let transaction = self.pool.begin().await?;
            sql!("DELETE FROM todo;")?.execute(&transaction).await?;
            let mut todos = Vec::new();
            for item in items {
                todos.push(
                    sql_as!("INSERT INTO todo (item) VALUES ({item}) RETURNING id, item;")?
                        .fetch_one(&transaction)
                        .await?,
                );
            }
            transaction.commit().await?;
            Ok(todos)
        })
    }

    /// Load every persisted todo.
    pub fn todos(&self) -> Result<Vec<Todo>> {
        block_on(sql_as!("SELECT id, item FROM todo ORDER BY id")?.fetch_all(&self.pool))
    }
}

#[cfg(test)]
mod tests {
    use futures::executor;

    use super::*;

    #[test]
    fn replace_todos_rolls_back_failed_insert() -> Result<()> {
        let store = Store::open(":memory:")?;
        let original = store.add_todo("original")?;
        block_on(
            musq::query(
                "CREATE TRIGGER reject_item BEFORE INSERT ON todo
            WHEN NEW.item = 'reject' BEGIN SELECT RAISE(ABORT, 'rejected'); END;",
            )
            .execute(&store.pool),
        )?;
        assert!(store.replace_todos(["first", "reject", "last"]).is_err());
        let rows = store.todos()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].id, rows[0].item.as_str()),
            (original.id, "original")
        );
        let replaced = store.replace_todos(["first", "last"])?;
        assert_eq!(
            replaced
                .iter()
                .map(|row| row.item.as_str())
                .collect::<Vec<_>>(),
            ["first", "last"]
        );
        assert_eq!(store.todos()?.len(), 2);
        assert!(store.replace_todos([])?.is_empty());
        assert!(store.todos()?.is_empty());
        Ok(())
    }

    #[test]
    fn todos_have_explicit_identifier_order() -> Result<()> {
        let store = Store::open(":memory:")?;
        store.replace_todos(["first", "second", "third"])?;
        block_on(musq::query("PRAGMA reverse_unordered_selects = ON;").execute(&store.pool))?;
        let rows = store.todos()?;
        assert_eq!(
            rows.iter().map(|row| row.item.as_str()).collect::<Vec<_>>(),
            ["first", "second", "third"]
        );
        assert!(rows.windows(2).all(|pair| pair[0].id < pair[1].id));
        Ok(())
    }

    #[test]
    fn failed_delete_preserves_selected_widget_and_storage() -> Result<()> {
        let store = Store::open(":memory:")?;
        let mut canopy = crate::create_app(store.clone(), None)?;
        canopy.apply_fixture("with_items")?;
        block_on(
            musq::query(
                "CREATE TRIGGER reject_delete BEFORE DELETE ON todo
            BEGIN SELECT RAISE(ABORT, 'cannot delete'); END;",
            )
            .execute(&store.pool),
        )?;
        let before = crate::with_todo(&mut canopy, |todo, ctx| {
            todo.with_list(ctx, |list, _| Ok((list.len(), list.selected_item())))
        })?;
        let original_rows = store.todos()?;
        assert!(crate::with_todo(&mut canopy, |todo, ctx| todo.delete_item(ctx)).is_err());
        let after = crate::with_todo(&mut canopy, |todo, ctx| {
            todo.with_list(ctx, |list, _| Ok((list.len(), list.selected_item())))
        })?;
        assert!(before.1.is_some());
        assert_eq!(before, after);
        assert_eq!(
            store
                .todos()?
                .iter()
                .map(|row| (row.id, row.item.clone()))
                .collect::<Vec<_>>(),
            original_rows
                .iter()
                .map(|row| (row.id, row.item.clone()))
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn todos_propagates_row_errors() -> Result<()> {
        let store = Store::open(":memory:")?;
        block_on(
            musq::query("INSERT INTO todo (id, item) VALUES (1, x'ff');").execute(&store.pool),
        )?;
        assert!(store.todos().is_err());
        Ok(())
    }

    #[test]
    fn file_store_preserves_records_across_reopens() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("todo.db");
        let path = path.to_str().unwrap();
        let original = {
            let store = Store::open(path)?;
            store.add_todo("Don't lose 🦀 or 'quotes'")?
        };
        let store = Store::open(path)?;
        let rows = store.todos()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, original.id);
        assert_eq!(rows[0].item, original.item);
        store.delete_todo(original.id)?;
        assert!(Store::open(path)?.todos()?.is_empty());
        Ok(())
    }

    #[test]
    fn clones_share_rows_but_memory_stores_are_isolated() -> Result<()> {
        let store = Store::open(":memory:")?;
        let cloned = store.clone();
        let row = store.add_todo("shared")?;
        drop(store);
        assert_eq!(cloned.todos()?[0].id, row.id);
        assert!(Store::open(":memory:")?.todos()?.is_empty());
        cloned.delete_todo(row.id)?;
        assert!(cloned.todos()?.is_empty());
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn storage_runs_inside_a_tokio_runtime() -> Result<()> {
        let store = Store::open(":memory:")?;
        store.replace_todos(["nested"])?;
        assert_eq!(store.todos()?[0].item, "nested");
        Ok(())
    }

    #[test]
    fn storage_runs_inside_a_futures_executor() -> Result<()> {
        executor::block_on(async {
            let store = Store::open(":memory:")?;
            store.replace_todos(["nested"])?;
            assert_eq!(store.todos()?[0].item, "nested");
            Ok(())
        })
    }
}
