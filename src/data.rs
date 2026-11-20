//! Pinned Raycast Chrome util/search-parser, bookmarks and read-only history behavior.
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub const MAX_JSON: u64 = 40 * 1024 * 1024;
pub fn read_json(path: &Path) -> Result<Value, String> {
    let f = fs::File::open(path).map_err(|e| format!("Cannot read Chrome data: {e}"))?;
    if !f.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Chrome data must be a regular file".into());
    }
    let mut bytes = Vec::new();
    f.take(MAX_JSON + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_JSON {
        return Err("Chrome JSON exceeds 40 MiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "Chrome JSON is malformed".into())
}
pub fn profile_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s != "."
        && s != ".."
        && !s.chars().any(|c| c == '/' || c == '\\' || c.is_control())
}
pub fn base_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(|p| PathBuf::from(p).join("Library/Application Support/Google/Chrome"))
        .ok_or("HOME is not set".into())
}
pub fn profile_dir(base: &Path, custom: &str, profile: &str) -> Result<PathBuf, String> {
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        if !p.is_absolute() {
            return Err("Profile Path must be absolute".into());
        }
        return Ok(p);
    }
    if !profile_id(profile) {
        return Err("Invalid Chrome profile ID".into());
    }
    Ok(base.join(profile))
}
pub fn profiles(base: &Path) -> Result<Vec<Value>, String> {
    if !base.join("Local State").exists() {
        return Ok(vec![json!({"id":"Default","name":"Default"})]);
    }
    let v = read_json(&base.join("Local State"))?;
    let info = v["profile"]["info_cache"]
        .as_object()
        .ok_or("Chrome Local State has no profiles")?;
    let r: Vec<_> = info
        .iter()
        .filter(|(k, _)| profile_id(k))
        .map(|(k, v)| json!({"id":k,"name":v["name"].as_str().unwrap_or(k)}))
        .collect();
    if r.is_empty() {
        Ok(vec![json!({"id":"Default","name":"Default"})])
    } else {
        Ok(r)
    }
}
pub fn terms(query: &str) -> (Vec<String>, Vec<String>) {
    let (mut yes, mut no) = (vec![], vec![]);
    for t in query.split_whitespace() {
        if let Some(t) = t.strip_prefix("\\-") {
            yes.push(format!("-{}", t.to_lowercase()))
        } else if let Some(t) = t.strip_prefix('-') {
            if !t.is_empty() {
                no.push(t.to_lowercase())
            }
        } else {
            yes.push(t.to_lowercase())
        }
    }
    (yes, no)
}
pub fn matches(text: &str, q: &str) -> bool {
    let text = text.to_lowercase();
    let (yes, no) = terms(q);
    yes.iter().all(|s| text.contains(s)) && !no.iter().any(|s| text.contains(s))
}
fn walk(v: &Value, out: &mut Vec<Value>, depth: usize) -> Result<(), String> {
    if depth > 64 {
        return Err("Chrome bookmark nesting exceeds 64".into());
    }
    if out.len() >= 100_000 {
        return Err("Chrome bookmarks exceed 100000".into());
    }
    if v["type"] == "url" {
        if let Some(url) = v["url"].as_str() {
            out.push(json!({"kind":"bookmark","id":v["id"],"title":v["name"].as_str().unwrap_or(url),"url":url,"lastVisited":v["date_added"]}));
        }
    } else if let Some(children) = v["children"].as_array() {
        for c in children {
            walk(c, out, depth + 1)?
        }
    }
    Ok(())
}
fn extract(path: &Path) -> Result<Vec<Value>, String> {
    let v = read_json(path)?;
    let mut r = vec![];
    for (_, v) in v["roots"]
        .as_object()
        .ok_or("Chrome bookmarks have no roots")?
    {
        walk(v, &mut r, 0)?
    }
    Ok(r)
}
pub fn bookmarks(profile: &Path, q: &str) -> Result<Vec<Value>, String> {
    let mut result = match extract(&profile.join("AccountBookmarks")) {
        Ok(v) if !v.is_empty() => v,
        _ => extract(&profile.join("Bookmarks"))?,
    };
    result.retain(|v| {
        matches(
            &format!(
                "{} {}",
                v["title"].as_str().unwrap_or(""),
                v["url"].as_str().unwrap_or("")
            ),
            q,
        )
    });
    Ok(result)
}
/// SQLITE_OPEN_READ_ONLY reads Chrome's WAL consistently; never creates or writes
/// browser database, copies nor a guessed immutable snapshot that ignores WAL.
pub fn history(profile: &Path, q: &str) -> Result<Vec<Value>, String> {
    let db = Connection::open_with_flags(
        profile.join("History"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Cannot open Chrome history read-only: {e}"))?;
    db.busy_timeout(Duration::from_millis(100))
        .map_err(|e| e.to_string())?;
    let (yes, no) = terms(q);
    let mut sql="SELECT id,url,title,datetime(last_visit_time/1000000+strftime('%s','1601-01-01'),'unixepoch','localtime') FROM urls WHERE last_visit_time>0".to_owned();
    let mut params = vec![];
    for (neg, terms) in [(false, yes), (true, no)] {
        for term in terms {
            sql.push_str(if neg {
                " AND NOT (title LIKE ? OR url LIKE ?)"
            } else {
                " AND (title LIKE ? OR url LIKE ?)"
            });
            params.push(format!("%{term}%"));
            params.push(format!("%{term}%"));
        }
    }
    sql.push_str(" ORDER BY last_visit_time DESC LIMIT 30");
    let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
    let rows=stmt.query_map(rusqlite::params_from_iter(params),|r|Ok(json!({"kind":"history","id":r.get::<_,i64>(0)?.to_string(),"url":r.get::<_,String>(1)?,"title":r.get::<_,String>(2)?,"lastVisited":r.get::<_,Option<String>>(3)?}))).map_err(|e|e.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}
pub fn new_tab_url(input: &str) -> String {
    let s = input.trim();
    if s.is_empty() {
        return "about:blank".into();
    }
    if (s.starts_with("https://") || s.starts_with("http://")) && url::Url::parse(s).is_ok() {
        return s.into();
    }
    let (ip, port) = s.split_once(':').map_or((s, None), |(a, b)| (a, Some(b)));
    let octets: Vec<_> = ip.split('.').collect();
    if octets.len() == 4
        && octets.iter().all(|s| {
            !s.is_empty()
                && s.len() <= 3
                && s.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<u16>().is_ok_and(|v| v <= 255)
        })
        && port.is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return format!("https://{s}");
    }
    let local = s.split(':').next().unwrap_or("");
    if local.eq_ignore_ascii_case("localhost")
        && s.split(':')
            .nth(1)
            .is_none_or(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        return format!("http://{s}");
    }
    if s.contains('.')
        && !s.chars().any(char::is_whitespace)
        && !s.ends_with('.')
        && s.rsplit('.').next().unwrap_or("").len() >= 2
        && !s.starts_with('.')
    {
        return format!("https://{s}");
    }
    format!(
        "https://www.google.com/search?q={}",
        tezbar_extension_sdk::encode_component(s)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_preserves_excludes_and_literal_dash() {
        assert!(matches("Raycast example -foo", "ray example \\-foo -bar"));
        assert!(!matches("Raycast firefox", "ray -firefox"));
        assert!(matches("anything", "-"));
    }
    #[test]
    fn account_bookmarks_preferred_and_invalid_fallback() {
        let d = tempfile::tempdir().unwrap();
        let b = json!({"roots":{"bar":{"type":"folder","children":[{"type":"url","id":"1","name":"Foo","url":"https://foo"}]}}});
        fs::write(d.path().join("Bookmarks"), b.to_string()).unwrap();
        fs::write(d.path().join("AccountBookmarks"), "invalid").unwrap();
        assert_eq!(bookmarks(d.path(), "foo").unwrap().len(), 1);
        let mut a = b;
        a["roots"]["bar"]["children"][0]["name"] = json!("Account");
        fs::write(d.path().join("AccountBookmarks"), a.to_string()).unwrap();
        assert_eq!(bookmarks(d.path(), "").unwrap()[0]["title"], "Account");
        assert!(bookmarks(d.path(), "-account").unwrap().is_empty());
    }
    #[test]
    fn history_filters_quotes_and_orders_without_writing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("History");
        let db = Connection::open(&p).unwrap();
        db.execute_batch("CREATE TABLE urls(id INTEGER,url TEXT,title TEXT,last_visit_time INTEGER);INSERT INTO urls VALUES(1,'https://a','O''Reilly',13000000000000000),(2,'https://b','Other',14000000000000000),(3,'https://c','Bookmark only',0);").unwrap();
        drop(db);
        let before = fs::read(&p).unwrap();
        assert_eq!(history(d.path(), "o'reilly").unwrap().len(), 1);
        assert!(history(d.path(), "' OR 1=1 --").unwrap().is_empty());
        assert_eq!(history(d.path(), "").unwrap()[0]["id"], "2");
        assert_eq!(fs::read(&p).unwrap(), before);
    }
    #[test]
    fn paths_reject_traversal() {
        assert!(!profile_id("../Default"));
        assert!(!profile_id(".."));
        assert!(profile_id("Profile 1"));
        assert!(profile_dir(Path::new("x"), "relative", "Default").is_err());
    }
    #[test]
    fn input_classification() {
        assert_eq!(new_tab_url("localhost:3000"), "http://localhost:3000");
        assert_eq!(new_tab_url("example.com"), "https://example.com");
        assert_eq!(new_tab_url("127.0.0.1:3000"), "https://127.0.0.1:3000");
        assert_eq!(
            new_tab_url("a & b"),
            "https://www.google.com/search?q=a%20%26%20b"
        );
    }
}
