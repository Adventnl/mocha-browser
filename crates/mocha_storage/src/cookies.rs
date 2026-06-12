//! Persistent cookie storage (Milestone 15).
//!
//! Cookies are stored in the `cookies` table (keyed by `name, domain, path`) and
//! matched/ordered for a request by loading them into a [`mocha_cookie::CookieJar`].
//! A private profile keeps cookies only in memory (the in-memory database), so a
//! fresh private profile has none.

use mocha_cookie::{Cookie, CookieJar, SameSite};
use mocha_error::MochaResult;
use mocha_url::Url;
use rusqlite::Connection;

use crate::storage_err;

/// Persists cookies in the profile database. Borrows the profile's connection.
pub struct CookieStore<'a> {
    conn: &'a Connection,
}

impl<'a> CookieStore<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Self {
        CookieStore { conn }
    }

    /// Insert or replace a cookie (keyed by name+domain+path), at `updated_ms`.
    /// An already-expired cookie deletes any existing match instead.
    pub fn set_cookie(&self, cookie: &Cookie, updated_ms: i64) -> MochaResult<()> {
        if cookie.is_expired(updated_ms) {
            self.conn
                .execute(
                    "DELETE FROM cookies WHERE name = ?1 AND domain = ?2 AND path = ?3",
                    rusqlite::params![cookie.name, cookie.domain, cookie.path],
                )
                .map_err(storage_err)?;
            return Ok(());
        }
        self.conn
            .execute(
                "INSERT INTO cookies
                   (name, value, domain, path, expires_ms, secure, http_only,
                    same_site, host_only, created_ms, updated_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(name, domain, path) DO UPDATE SET
                    value = excluded.value,
                    expires_ms = excluded.expires_ms,
                    secure = excluded.secure,
                    http_only = excluded.http_only,
                    same_site = excluded.same_site,
                    host_only = excluded.host_only,
                    updated_ms = excluded.updated_ms",
                rusqlite::params![
                    cookie.name,
                    cookie.value,
                    cookie.domain,
                    cookie.path,
                    cookie.expires_ms,
                    cookie.secure as i64,
                    cookie.http_only as i64,
                    cookie.same_site.as_str(),
                    cookie.host_only as i64,
                    cookie.created_ms,
                    updated_ms,
                ],
            )
            .map_err(storage_err)?;
        Ok(())
    }

    /// Parse and store a `Set-Cookie` header for `request_url`.
    pub fn store_set_cookie(
        &self,
        header: &str,
        request_url: &Url,
        now_ms: i64,
    ) -> MochaResult<()> {
        let cookie = mocha_cookie::parse_set_cookie(header, request_url, now_ms)?;
        self.set_cookie(&cookie, now_ms)
    }

    /// All non-expired, matching cookies for `url`, in send order.
    pub fn cookies_for_request(&self, url: &Url, now_ms: i64) -> MochaResult<Vec<Cookie>> {
        let jar = self.load_jar()?;
        Ok(jar.cookies_for_request(url, now_ms))
    }

    /// The `Cookie` request header for `url`, or `None`.
    pub fn cookie_header_for_request(&self, url: &Url, now_ms: i64) -> MochaResult<Option<String>> {
        let jar = self.load_jar()?;
        Ok(jar.cookie_header_for_request(url, now_ms))
    }

    /// Delete all cookies.
    pub fn clear_cookies(&self) -> MochaResult<()> {
        self.conn
            .execute("DELETE FROM cookies", [])
            .map_err(storage_err)?;
        Ok(())
    }

    /// Load every stored cookie into an in-memory jar for matching.
    fn load_jar(&self) -> MochaResult<CookieJar> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT name, value, domain, path, expires_ms, secure, http_only,
                        same_site, host_only, created_ms
                 FROM cookies",
            )
            .map_err(storage_err)?;
        let rows = stmt
            .query_map([], |row| {
                let same_site: String = row.get(7)?;
                Ok(Cookie {
                    name: row.get(0)?,
                    value: row.get(1)?,
                    domain: row.get(2)?,
                    path: row.get(3)?,
                    expires_ms: row.get(4)?,
                    max_age: None,
                    secure: row.get::<_, i64>(5)? != 0,
                    http_only: row.get::<_, i64>(6)? != 0,
                    same_site: SameSite::from_storage(&same_site),
                    host_only: row.get::<_, i64>(8)? != 0,
                    created_ms: row.get(9)?,
                })
            })
            .map_err(storage_err)?;
        let cookies = rows.collect::<Result<Vec<_>, _>>().map_err(storage_err)?;
        Ok(CookieJar::from_cookies(cookies))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use crate::Profile;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn set_and_retrieve_cookie() {
        let p = Profile::private().unwrap();
        let c = p.cookies();
        c.store_set_cookie("sid=abc", &url("http://e.com/"), 0)
            .unwrap();
        assert_eq!(
            c.cookie_header_for_request(&url("http://e.com/"), 0)
                .unwrap()
                .as_deref(),
            Some("sid=abc")
        );
    }

    #[test]
    fn replace_same_name_domain_path() {
        let p = Profile::private().unwrap();
        let c = p.cookies();
        c.store_set_cookie("a=1", &url("http://e.com/"), 0).unwrap();
        c.store_set_cookie("a=2", &url("http://e.com/"), 0).unwrap();
        assert_eq!(
            c.cookies_for_request(&url("http://e.com/"), 0)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            c.cookie_header_for_request(&url("http://e.com/"), 0)
                .unwrap()
                .as_deref(),
            Some("a=2")
        );
    }

    #[test]
    fn expired_cookie_not_returned() {
        let p = Profile::private().unwrap();
        let c = p.cookies();
        c.store_set_cookie("a=b; Max-Age=10", &url("http://e.com/"), 1000)
            .unwrap();
        assert!(c
            .cookie_header_for_request(&url("http://e.com/"), 999_999)
            .unwrap()
            .is_none());
    }

    #[test]
    fn clear_cookies_empties_store() {
        let p = Profile::private().unwrap();
        let c = p.cookies();
        c.store_set_cookie("a=b", &url("http://e.com/"), 0).unwrap();
        c.clear_cookies().unwrap();
        assert!(c
            .cookies_for_request(&url("http://e.com/"), 0)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cookies_persist_across_reopen_but_private_does_not() {
        let dir = TempDir::new();
        {
            let p = Profile::persistent(dir.path()).unwrap();
            p.cookies()
                .store_set_cookie("sid=abc", &url("http://e.com/"), 0)
                .unwrap();
        }
        let p = Profile::persistent(dir.path()).unwrap();
        assert!(p
            .cookies()
            .cookie_header_for_request(&url("http://e.com/"), 0)
            .unwrap()
            .is_some());

        // Private cookies never persist.
        let pr1 = Profile::private().unwrap();
        pr1.cookies()
            .store_set_cookie("sid=abc", &url("http://e.com/"), 0)
            .unwrap();
        let pr2 = Profile::private().unwrap();
        assert!(pr2
            .cookies()
            .cookie_header_for_request(&url("http://e.com/"), 0)
            .unwrap()
            .is_none());
    }
}
