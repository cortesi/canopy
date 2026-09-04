//! SQLite storage for the todo example.

use std::rc::Rc;

use anyhow::Result;
use rusqlite::Connection;

#[derive(Debug, Clone)]
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
    /// Shared connection for cloned store handles on one thread.
    conn: Rc<Connection>,
}

impl Store {
    /// Open or initialize a SQLite store.
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS todo (
                id INTEGER PRIMARY KEY,
                item TEXT NOT NULL
            );",
            rusqlite::params![],
        )?;
        Ok(Self {
            conn: Rc::new(conn),
        })
    }

    /// Insert a todo and return its persisted record.
    pub(crate) fn add_todo(&self, item: &str) -> Result<Todo> {
        self.conn.execute(
            "INSERT INTO todo (item) VALUES (?1);",
            rusqlite::params![item],
        )?;
        Ok(Todo {
            id: self.conn.last_insert_rowid(),
            item: item.to_string(),
        })
    }

    /// Delete a todo by database identifier.
    pub(crate) fn delete_todo(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM todo WHERE id=?1;", rusqlite::params![id])?;
        Ok(())
    }

    /// Delete every todo in the store.
    pub(crate) fn clear_todos(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM todo;", rusqlite::params![])?;
        Ok(())
    }

    /// Replace all todos and return their new persisted records.
    pub(crate) fn replace_todos<'a>(
        &self,
        items: impl IntoIterator<Item = &'a str>,
    ) -> Result<Vec<Todo>> {
        let transaction = self.conn.unchecked_transaction()?;
        self.clear_todos()?;
        let mut todos = Vec::new();
        for item in items {
            todos.push(self.add_todo(item)?);
        }
        transaction.commit()?;
        Ok(todos)
    }

    /// Load every persisted todo.
    pub fn todos(&self) -> Result<Vec<Todo>> {
        let mut stmt = self.conn.prepare("SELECT id, item FROM todo ORDER BY id")?;
        let todos = stmt
            .query_map([], |row| {
                Ok(Todo {
                    id: row.get(0)?,
                    item: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(todos)
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use rusqlite::Connection;

    use super::*;

    #[test]
    fn replace_todos_rolls_back_failed_insert() -> Result<()> {
        let store = Store::open(":memory:")?;
        let original = store.add_todo("original")?;
        store.conn.execute_batch(
            "CREATE TRIGGER reject_item BEFORE INSERT ON todo
            WHEN NEW.item = 'reject' BEGIN SELECT RAISE(ABORT, 'rejected'); END;",
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
        store
            .conn
            .execute_batch("PRAGMA reverse_unordered_selects = ON;")?;
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
        let mut canopy = crate::create_app_with_store(store.clone(), None)?;
        canopy.apply_fixture("with_items")?;
        store.conn.execute_batch(
            "CREATE TRIGGER reject_delete BEFORE DELETE ON todo
            BEGIN SELECT RAISE(ABORT, 'cannot delete'); END;",
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
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE todo (
                id INTEGER PRIMARY KEY,
                item BLOB NOT NULL
            );",
            [],
        )?;
        conn.execute("INSERT INTO todo (id, item) VALUES (1, x'ff');", [])?;

        let store = Store {
            conn: Rc::new(conn),
        };
        assert!(store.todos().is_err());
        Ok(())
    }
}
