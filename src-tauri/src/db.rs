#![allow(unused)]
use rusqlite::Connection;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let conn = Connection::open("./db/metadata.sqlite3").unwrap();
        conn.execute(include_str!("../db/schema.sql"), ()).unwrap();
        let mut stmt = conn
            .prepare("SELECT base_dir FROM cases WHERE id > ?")
            .unwrap();
        let mut rows = stmt.query([0]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            let base_dir: String = row.get(0).unwrap();
            println!("{}", base_dir);
        }
    }
}
