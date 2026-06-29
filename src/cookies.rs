use std::path::{Path, PathBuf};
use std::time::SystemTime;

const COOKIE_NAME: &str = "_simpleauth_sess";

fn firefox_roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };

    [
        home.join(".mozilla/firefox"),
        home.join("snap/firefox/common/.mozilla/firefox"),
        home.join(".var/app/org.mozilla.firefox/.mozilla/firefox"),
    ]
    .into_iter()
    .filter(|path| path.is_dir())
    .collect()
}

fn newest_cookies_db() -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;

    for root in firefox_roots() {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };

        for entry in entries.flatten() {
            let db = entry.path().join("cookies.sqlite");
            let Ok(mtime) = db.metadata().and_then(|metadata| metadata.modified()) else {
                continue;
            };

            if best.as_ref().is_none_or(|(time, _)| mtime > *time) {
                best = Some((mtime, db));
            }
        }
    }

    best.map(|(_, db)| db)
}

fn copy_if_exists(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.exists() {
        std::fs::copy(from, to)?;
    }
    Ok(())
}

fn read_humble_token(db: &Path) -> rusqlite::Result<Option<String>> {
    let dir = tempfile::tempdir()
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
    let tmp_db = dir.path().join("cookies.sqlite");
    std::fs::copy(db, &tmp_db)
        .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
    copy_if_exists(
        &db.with_extension("sqlite-wal"),
        &tmp_db.with_extension("sqlite-wal"),
    )
    .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;
    copy_if_exists(
        &db.with_extension("sqlite-shm"),
        &tmp_db.with_extension("sqlite-shm"),
    )
    .map_err(|err| rusqlite::Error::ToSqlConversionFailure(Box::new(err)))?;

    let conn =
        rusqlite::Connection::open_with_flags(tmp_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    conn.query_row(
        "SELECT value
           FROM moz_cookies
          WHERE name = ?1
            AND host LIKE '%humblebundle.com'
          ORDER BY lastAccessed DESC
          LIMIT 1",
        [COOKIE_NAME],
        |row| row.get(0),
    )
    .map(Some)
    .or_else(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        err => Err(err),
    })
}

pub fn load() -> Option<String> {
    let db = newest_cookies_db()?;
    match read_humble_token(&db) {
        Ok(token) => token,
        Err(err) => {
            eprintln!("[cookies] reading {} failed: {err}", db.display());
            None
        }
    }
}
