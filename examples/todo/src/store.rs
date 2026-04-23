use std::{cell::RefCell, path::Path, rc::Rc};

use anyhow::{Result, anyhow};
use rusqlite::Connection;

thread_local! {
    static STORE: RefCell<Option<Store>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone)]
pub struct Todo {
    pub id: i64,
    pub item: String,
}

#[derive(Debug, Clone)]
pub struct Store {
    conn: Rc<Connection>,
}

impl Store {
    fn open(path: &str) -> Result<Self> {
        let conn = if Path::new(path).is_file() {
            Connection::open(path)?
        } else {
            let conn = Connection::open(path)?;
            conn.execute(
                "CREATE TABLE todo (
                    id INTEGER PRIMARY KEY,
                    item TEXT NOT NULL
                );",
                rusqlite::params![],
            )?;
            conn
        };
        Ok(Store {
            conn: Rc::new(conn),
        })
    }

    pub fn add_todo(&self, item: &str) -> Result<Todo> {
        self.conn.execute(
            "INSERT INTO todo (item) VALUES (?1);",
            rusqlite::params![item],
        )?;
        Ok(Todo {
            id: self.conn.last_insert_rowid(),
            item: item.to_string(),
        })
    }

    pub fn delete_todo(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM todo WHERE id=?1;", rusqlite::params![id])?;
        Ok(())
    }

    pub fn clear_todos(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM todo;", rusqlite::params![])?;
        Ok(())
    }

    pub fn replace_todos<'a>(&self, items: impl IntoIterator<Item = &'a str>) -> Result<Vec<Todo>> {
        self.clear_todos()?;
        let mut todos = Vec::new();
        for item in items {
            todos.push(self.add_todo(item)?);
        }
        Ok(todos)
    }

    pub fn todos(&self) -> Result<Vec<Todo>> {
        let mut stmt = self.conn.prepare("SELECT id, item FROM todo")?;
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

pub fn open(path: &str) -> Result<()> {
    let s = Store::open(path)?;
    STORE.with(|store| {
        *store.borrow_mut() = Some(s);
    });
    Ok(())
}

pub fn get() -> Result<Store> {
    STORE.with(|store| {
        store
            .borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow!("todo store has not been opened"))
    })
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use rusqlite::Connection;

    use super::*;

    #[test]
    fn get_errors_before_open() {
        STORE.with(|store| {
            *store.borrow_mut() = None;
        });
        assert!(get().is_err());
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
