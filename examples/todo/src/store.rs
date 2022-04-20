use std::{cell::RefCell, path::Path, rc::Rc};

use anyhow::Result;
use rusqlite::Connection;

thread_local! {
    pub static STORE: RefCell<Option<Store>> = RefCell::new(None);
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

    pub fn todos(&self) -> Result<Vec<Todo>> {
        let mut stmt = self.conn.prepare("SELECT id, item FROM todo")?;
        let ret = stmt
            .query_map([], |row| {
                Ok(Todo {
                    id: row.get(0)?,
                    item: row.get(1)?,
                })
            })?
            .map(|x| x.unwrap())
            .collect();
        Ok(ret)
    }
}

pub fn open(path: &str) -> Result<()> {
    let s = Store::open(path)?;
    STORE.with(|store| {
        *store.borrow_mut() = Some(s);
    });
    Ok(())
}

pub fn get() -> Store {
    STORE.with(|store| store.borrow_mut().as_mut().unwrap().clone())
}
