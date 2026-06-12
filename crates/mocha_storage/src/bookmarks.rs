//! Bookmark storage.

use mocha_error::MochaResult;
use mocha_url::Url;
use rusqlite::Connection;

use crate::storage_err;

/// One bookmark row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookmarkEntry {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub created_ms: i64,
    pub updated_ms: i64,
}

/// Stores and lists bookmarks. Borrows the profile's connection.
pub struct BookmarkStore<'a> {
    conn: &'a Connection,
}

impl<'a> BookmarkStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        BookmarkStore { conn }
    }

    /// Add a bookmark and return the created entry (with its assigned id).
    pub fn add_bookmark(&self, url: &Url, title: &str, now_ms: i64) -> MochaResult<BookmarkEntry> {
        let key = url.normalized();
        self.conn
            .execute(
                "INSERT INTO bookmarks (url, title, created_ms, updated_ms)
                 VALUES (?1, ?2, ?3, ?3)",
                rusqlite::params![key, title, now_ms],
            )
            .map_err(storage_err)?;
        Ok(BookmarkEntry {
            id: self.conn.last_insert_rowid(),
            url: key,
            title: title.to_string(),
            created_ms: now_ms,
            updated_ms: now_ms,
        })
    }

    /// Remove a bookmark by id. Returns whether a row was deleted.
    pub fn remove_bookmark(&self, id: i64) -> MochaResult<bool> {
        let affected = self
            .conn
            .execute("DELETE FROM bookmarks WHERE id = ?1", [id])
            .map_err(storage_err)?;
        Ok(affected > 0)
    }

    /// List all bookmarks, oldest first.
    pub fn list_bookmarks(&self) -> MochaResult<Vec<BookmarkEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, url, title, created_ms, updated_ms
                 FROM bookmarks ORDER BY created_ms ASC, id ASC",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BookmarkEntry {
                    id: row.get(0)?,
                    url: row.get(1)?,
                    title: row.get(2)?,
                    created_ms: row.get(3)?,
                    updated_ms: row.get(4)?,
                })
            })
            .map_err(storage_err)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(storage_err)
    }

    /// Update a bookmark's title and `updated_ms`.
    pub fn update_title(&self, id: i64, title: &str, now_ms: i64) -> MochaResult<()> {
        self.conn
            .execute(
                "UPDATE bookmarks SET title = ?2, updated_ms = ?3 WHERE id = ?1",
                rusqlite::params![id, title, now_ms],
            )
            .map_err(storage_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Profile;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn add_list_and_remove() {
        let p = Profile::private().unwrap();
        let b = p.bookmarks();
        let entry = b.add_bookmark(&url("http://a.com/"), "A", 10).unwrap();
        assert_eq!(b.list_bookmarks().unwrap().len(), 1);
        assert_eq!(entry.title, "A");
        assert!(b.remove_bookmark(entry.id).unwrap());
        assert!(b.list_bookmarks().unwrap().is_empty());
        // Removing a missing id returns false.
        assert!(!b.remove_bookmark(999).unwrap());
    }

    #[test]
    fn update_title_changes_it() {
        let p = Profile::private().unwrap();
        let b = p.bookmarks();
        let entry = b.add_bookmark(&url("http://a.com/"), "A", 10).unwrap();
        b.update_title(entry.id, "B", 20).unwrap();
        let listed = b.list_bookmarks().unwrap();
        assert_eq!(listed[0].title, "B");
        assert_eq!(listed[0].updated_ms, 20);
        assert_eq!(listed[0].created_ms, 10);
    }

    #[test]
    fn private_bookmarks_not_persisted() {
        let p1 = Profile::private().unwrap();
        p1.bookmarks()
            .add_bookmark(&url("http://a.com/"), "A", 10)
            .unwrap();
        let p2 = Profile::private().unwrap();
        assert!(p2.bookmarks().list_bookmarks().unwrap().is_empty());
    }
}
