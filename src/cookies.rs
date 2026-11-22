use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

/// Builds a `Cookie` header from every non-expired humblebundle.com cookie.
fn read_humble_cookies(db: &Path) -> rusqlite::Result<Option<String>> {
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
    let mut stmt = conn.prepare(
        "SELECT name, value
           FROM moz_cookies
          WHERE host LIKE '%humblebundle.com'
            AND (expiry = 0 OR expiry > strftime('%s', 'now'))
          ORDER BY lastAccessed DESC",
    )?;
    let mut seen = std::collections::HashSet::new();
    let pairs: Vec<String> = stmt
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))?
        .filter_map(Result::ok)
        .filter(|(name, _)| seen.insert(name.clone()))
        .map(|(name, value)| format!("{name}={value}"))
        .collect();

    Ok((!pairs.is_empty()).then(|| pairs.join("; ")))
}

pub fn load() -> Option<String> {
    let db = newest_cookies_db()?;
    match read_humble_cookies(&db) {
        Ok(cookies) => cookies,
        Err(err) => {
            eprintln!("[cookies] reading {} failed: {err}", db.display());
            None
        }
    }
}
