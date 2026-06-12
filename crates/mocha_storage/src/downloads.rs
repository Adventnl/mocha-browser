//! Downloads-metadata storage.
//!
//! Milestone 14 stores only download *metadata*; there is no actual downloader.

use mocha_error::{MochaError, MochaResult};
use mocha_url::Url;
use rusqlite::Connection;

use crate::storage_err;

/// The lifecycle state of a download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    InProgress,
    Complete,
    Failed,
    Canceled,
}

impl DownloadStatus {
    fn as_str(self) -> &'static str {
        match self {
            DownloadStatus::InProgress => "in_progress",
            DownloadStatus::Complete => "complete",
            DownloadStatus::Failed => "failed",
            DownloadStatus::Canceled => "canceled",
        }
    }

    fn from_str(s: &str) -> MochaResult<DownloadStatus> {
        match s {
            "in_progress" => Ok(DownloadStatus::InProgress),
            "complete" => Ok(DownloadStatus::Complete),
            "failed" => Ok(DownloadStatus::Failed),
            "canceled" => Ok(DownloadStatus::Canceled),
            other => Err(MochaError::Storage(format!(
                "unknown download status {other:?}"
            ))),
        }
    }
}

/// One download row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadEntry {
    pub id: i64,
    pub url: String,
    pub target_path: String,
    pub started_ms: i64,
    pub finished_ms: Option<i64>,
    pub status: DownloadStatus,
}

/// Stores download metadata. Borrows the profile's connection.
pub struct DownloadStore<'a> {
    conn: &'a Connection,
}

impl<'a> DownloadStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        DownloadStore { conn }
    }

    /// Insert a new in-progress download; returns its id.
    pub fn insert_download(&self, url: &Url, target_path: &str, now_ms: i64) -> MochaResult<i64> {
        self.conn
            .execute(
                "INSERT INTO downloads (url, target_path, started_ms, finished_ms, status)
                 VALUES (?1, ?2, ?3, NULL, ?4)",
                rusqlite::params![
                    url.normalized(),
                    target_path,
                    now_ms,
                    DownloadStatus::InProgress.as_str()
                ],
            )
            .map_err(storage_err)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Mark a download complete at `now_ms`.
    pub fn mark_complete(&self, id: i64, now_ms: i64) -> MochaResult<()> {
        self.finish(id, DownloadStatus::Complete, now_ms)
    }

    /// Mark a download failed at `now_ms`.
    pub fn mark_failed(&self, id: i64, now_ms: i64) -> MochaResult<()> {
        self.finish(id, DownloadStatus::Failed, now_ms)
    }

    /// Mark a download canceled at `now_ms`.
    pub fn mark_canceled(&self, id: i64, now_ms: i64) -> MochaResult<()> {
        self.finish(id, DownloadStatus::Canceled, now_ms)
    }

    fn finish(&self, id: i64, status: DownloadStatus, now_ms: i64) -> MochaResult<()> {
        self.conn
            .execute(
                "UPDATE downloads SET status = ?2, finished_ms = ?3 WHERE id = ?1",
                rusqlite::params![id, status.as_str(), now_ms],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    /// List downloads, most recently started first.
    pub fn list_downloads(&self) -> MochaResult<Vec<DownloadEntry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, url, target_path, started_ms, finished_ms, status
                 FROM downloads ORDER BY started_ms DESC, id DESC",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |row| {
                let status_text: String = row.get(5)?;
                Ok((
                    DownloadEntry {
                        id: row.get(0)?,
                        url: row.get(1)?,
                        target_path: row.get(2)?,
                        started_ms: row.get(3)?,
                        finished_ms: row.get(4)?,
                        status: DownloadStatus::InProgress, // placeholder; set below
                    },
                    status_text,
                ))
            })
            .map_err(storage_err)?;
        let mut out = Vec::new();
        for row in rows {
            let (mut entry, status_text) = row.map_err(storage_err)?;
            entry.status = DownloadStatus::from_str(&status_text)?;
            out.push(entry);
        }
        Ok(out)
    }

    /// Delete all download metadata.
    pub fn clear_downloads(&self) -> MochaResult<()> {
        self.conn
            .execute("DELETE FROM downloads", [])
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
    fn insert_and_complete() {
        let p = Profile::private().unwrap();
        let d = p.downloads();
        let id = d
            .insert_download(&url("http://a.com/f.bin"), "/tmp/f.bin", 100)
            .unwrap();
        d.mark_complete(id, 200).unwrap();
        let list = d.list_downloads().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].status, DownloadStatus::Complete);
        assert_eq!(list[0].finished_ms, Some(200));
    }

    #[test]
    fn mark_failed_and_canceled() {
        let p = Profile::private().unwrap();
        let d = p.downloads();
        let a = d.insert_download(&url("http://a/1"), "/t/1", 10).unwrap();
        let b = d.insert_download(&url("http://a/2"), "/t/2", 20).unwrap();
        d.mark_failed(a, 11).unwrap();
        d.mark_canceled(b, 21).unwrap();
        let list = d.list_downloads().unwrap();
        // Newest started first: b (20) then a (10).
        assert_eq!(list[0].status, DownloadStatus::Canceled);
        assert_eq!(list[1].status, DownloadStatus::Failed);
    }

    #[test]
    fn clear_downloads_empties_it() {
        let p = Profile::private().unwrap();
        let d = p.downloads();
        d.insert_download(&url("http://a/1"), "/t/1", 10).unwrap();
        d.clear_downloads().unwrap();
        assert!(d.list_downloads().unwrap().is_empty());
    }
}
