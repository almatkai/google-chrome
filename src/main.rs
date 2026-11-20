//! Standalone native Google Chrome port; upstream f8e165c85832e16d9c24152bbb98b4871b954fc8.
mod browser;
mod data;
use serde_json::{json, Value};
use tezbar_extension_sdk::{self as sdk, BoxFuture, CoreResult, Extension, ExtensionContext};
const COMMANDS: &[&str] = &[
    "new-tab",
    "search-tab",
    "search-history",
    "search-bookmarks",
    "search-all",
    "new-window",
    "new-incognito-window",
    "new-guest-window",
    "open-current-in-guest",
    "name-window",
    "search-windows",
];
struct Chrome;
static NEXT_REVISION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}
fn rank(v: &Value, now: f64) -> f64 {
    let count = v["count"].as_f64().unwrap_or(0.);
    let last = v["lastVisited"].as_f64().unwrap_or(now);
    count * 0.5_f64.powf(((now - last).max(0.)) / 3_600_000. / 72.)
}
fn profile_directory(
    command: &str,
    state: &mut Value,
    base: impl FnOnce() -> Result<std::path::PathBuf, String>,
) -> Result<Option<std::path::PathBuf>, String> {
    if ![
        "new-tab",
        "search-history",
        "search-bookmarks",
        "search-all",
    ]
    .contains(&command)
    {
        state["profiles"] = json!([]);
        return Ok(None);
    }
    let base = base()?;
    let profiles = data::profiles(&base)?;
    let mut profile = text(state, "profile").to_owned();
    if !profiles.iter().any(|p| p["id"] == profile) {
        profile = profiles
            .first()
            .map(|p| text(p, "id"))
            .unwrap_or("Default")
            .into();
    }
    state["profile"] = json!(profile);
    state["profiles"] = json!(profiles);
    let custom = text(&state["preferences"], "profilePath");
    data::profile_dir(&base, custom, &profile).map(Some)
}
async fn load(ctx: &ExtensionContext, command: &str, state: &mut Value) -> Result<(), String> {
    let q = text(state, "query").to_owned();
    if q.len() > 4096 {
        return Err("Search is limited to 4096 bytes".into());
    }
    let dir = profile_directory(command, state, data::base_dir)?;
    let mut items = vec![];
    let mut errors = vec![];
    if ["new-tab", "search-tab", "search-all"].contains(&command) {
        match browser::run(json!({"kind":"tabs","favicon":state["preferences"]["useOriginalFavicon"].as_bool().unwrap_or(false)})).await{
   Ok(v)=>for i in v.as_array().ok_or("Chrome tabs response is not an array")?{let query=if command=="new-tab"{""}else{&q};let raw=text(i,"url");let url=raw.split_once("://").map_or(raw,|(_,v)|v).trim_start_matches("//").replacen("www.","",1);if data::matches(&format!("{} {}",text(i,"title"),url),query){items.push(i.clone())}},Err(e)=>errors.push(e)
  }
    }
    if command == "search-windows" {
        match browser::run(json!({"kind":"windows"})).await {
            Ok(v) => {
                for i in v
                    .as_array()
                    .ok_or("Chrome windows response is not an array")?
                {
                    if text(i, "title").to_lowercase().contains(&q.to_lowercase()) {
                        items.push(i.clone())
                    }
                }
            }
            Err(e) => errors.push(e),
        }
    }
    if ["new-tab", "search-history", "search-all"].contains(&command) {
        match data::history(dir.as_deref().ok_or("Missing Chrome profile")?, &q) {
            Ok(v) => items.extend(v),
            Err(e) => errors.push(e),
        }
    }
    if ["search-bookmarks", "search-all"].contains(&command) {
        match data::bookmarks(dir.as_deref().ok_or("Missing Chrome profile")?, &q) {
            Ok(mut v) => {
                let store = sdk::read_store(ctx, "chrome").await?;
                let now = now_ms();
                v.sort_by(|a, b| {
                    rank(&store["bookmarks"][text(b, "url")], now)
                        .total_cmp(&rank(&store["bookmarks"][text(a, "url")], now))
                });
                items.extend(v);
            }
            Err(e) => errors.push(e),
        }
    }
    state["items"] = json!(items);
    state["errors"] = json!(errors);
    Ok(())
}
fn add(actions: &mut Vec<Value>, op: &str, title: &str, rev: u64, index: usize) -> String {
    let id = format!("{op}:{rev}:{index}");
    actions.push(json!({"id":id,"title":title,"kind":"callback"}));
    id
}
fn render(command: &str, state: &mut Value) -> Result<Value, String> {
    // Process-wide revisions prevent actions from another session matching a row.
    let rev = NEXT_REVISION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    state["revision"] = json!(rev);
    let limit = state["limit"].as_u64().unwrap_or(100).min(10000) as usize;
    let mut actions = vec![];
    let mut children = vec![];
    if command == "new-tab" {
        let id = add(&mut actions, "new", "Open New Tab", rev, 0);
        children.push(json!({"type":"List.Item","props":{"id":"new","title":if text(state,"query").is_empty(){"Open Empty Tab".to_owned()}else{format!("Open or Search: {}",text(state,"query"))},"actionIds":[id]}}));
    }
    for (i, item) in state["items"]
        .as_array()
        .into_iter()
        .flatten()
        .take(limit)
        .enumerate()
    {
        let kind = text(item, "kind");
        let mut ids = vec![];
        let op = if kind == "tab" {
            "focus-tab"
        } else if kind == "window" {
            "focus-window"
        } else {
            "open"
        };
        ids.push(add(&mut actions, op, "Open", rev, i));
        if kind == "tab" {
            ids.push(add(&mut actions, "reload-tab", "Reload Tab", rev, i));
            ids.push(add(&mut actions, "close-tab", "Close Tab", rev, i));
            ids.push(add(&mut actions, "quicklink", "Create Quick Link", rev, i));
        }
        if kind != "window" {
            ids.push(add(&mut actions, "guest", "Open in Guest Window", rev, i));
        }
        if kind == "history" || kind == "bookmark" {
            ids.push(add(
                &mut actions,
                "current",
                "Open in Current Profile",
                rev,
                i,
            ));
            ids.push(add(
                &mut actions,
                "original",
                "Open in Original Profile",
                rev,
                i,
            ));
        }
        ids.push(add(&mut actions, "copy-url", "Copy URL", rev, i));
        ids.push(add(&mut actions, "copy-title", "Copy Title", rev, i));
        if kind == "bookmark" {
            ids.push(add(&mut actions, "reset", "Reset Ranking", rev, i));
        }
        let mut accessories = vec![json!({"tag":kind})];
        if kind == "history" {
            accessories.push(json!({"text":item["lastVisited"]}));
        }
        let mut props = json!({"id":format!("item:{i}"),"title":text(item,"title"),"subtitle":text(item,"url"),"accessories":accessories,"actionIds":ids});
        if kind == "tab"
            && state["preferences"]["useOriginalFavicon"] == true
            && !text(item, "favicon").is_empty()
        {
            props["icon"] = json!({"source":text(item,"favicon")});
        }
        children.push(json!({"type":"List.Item","props":props}));
    }
    for (i, e) in state["errors"].as_array().into_iter().flatten().enumerate() {
        children.push(json!({"type":"List.Item","props":{"id":format!("error:{i}"),"title":"Could not load Chrome data","subtitle":e,"actionIds":[format!("refresh:{rev}:0")]}}));
    }
    let count = state["items"].as_array().map_or(0, Vec::len);
    if limit == 10000 && count > limit {
        children.push(json!({"type":"List.Item","props":{"id":"limit-reached","title":"Showing the first 10,000 results","subtitle":"Narrow your search to find more matches."}}));
    }
    let refresh = add(&mut actions, "refresh", "Refresh", rev, 0);
    if children.is_empty() {
        children.push(json!({"type":"List.EmptyView","props":{"title":"No results","description":"Try another search or profile."}}));
    }
    let profiles = state["profiles"].as_array().cloned().unwrap_or_default();
    let mut props = json!({"filtering":false,"__hasServerSearch":true,"searchText":state["query"],"searchBarPlaceholder":"Search Chrome","actionIds":[refresh]});
    if profiles.len() > 1 {
        props["searchBarAccessory"] = json!({"type":"List.Dropdown","props":{"value":state["profile"],"actionId":format!("profile:{rev}:0"),"tooltip":"Select Chrome Profile"},"children":profiles.iter().map(|p|json!({"type":"List.Dropdown.Item","props":{"title":p["name"],"value":p["id"]}})).collect::<Vec<_>>()});
    }
    let mut view = sdk::view(
        "Google Chrome",
        json!({"type":"List","props":props,"children":children}),
        actions,
    );
    let count = state["items"].as_array().map_or(0, Vec::len);
    view["hasMore"] = json!(count > limit && limit < 10000);
    if serde_json::to_vec(&view).map_err(|e| e.to_string())?.len() > 8 * 1024 * 1024 {
        return Err("Chrome results exceed 8 MiB; narrow the search".into());
    }
    Ok(view)
}
async fn invoke(ctx: &ExtensionContext, command: &str, p: &Value, state: &mut Value) -> CoreResult {
    let id = text(p, "actionId");
    let parts: Vec<_> = id.split(':').collect();
    if parts.len() != 3 || parts[1].parse::<u64>().ok() != state["revision"].as_u64() {
        return Err("Stale Chrome action; refresh".into());
    }
    let op = parts[0];
    let i: usize = parts[2].parse().map_err(|_| "Invalid Chrome action")?;
    if op == "profile" {
        let value = p["formValues"]["value"]
            .as_str()
            .ok_or("Missing profile selection")?;
        if !state["profiles"]
            .as_array()
            .is_some_and(|a| a.iter().any(|p| p["id"] == value))
        {
            return Err("Unknown Chrome profile".into());
        }
        let selected = value.to_owned();
        sdk::update_store(ctx, "chrome", move |s| {
            s["profile"] = json!(selected);
            Ok(())
        })
        .await?;
        state["profile"] = json!(value);
    } else if op == "refresh" {
    } else if op == "new" && command == "new-tab" {
        let target = data::new_tab_url(text(state, "query"));
        browser::new_tab(
            &target,
            text(&state["preferences"], "openTabInProfile"),
            text(state, "profile"),
            text(state, "profile"),
        )
        .await?;
        hide(ctx).await?;
        return Ok(sdk::no_view("Opened Chrome tab"));
    } else {
        let item = state["items"]
            .get(i)
            .cloned()
            .ok_or("Unknown Chrome result")?;
        let kind = text(&item, "kind");
        let url = text(&item, "url");
        match op {
            "quicklink" if kind == "tab" => {
                let result = ctx
                    .host(
                        "host.quicklinks.create",
                        json!({"name":text(&item,"title"),"link":url}),
                    )
                    .await?;
                if result["status"] != "created" && result["status"] != "cancelled" {
                    return Err("The host did not confirm Quick Link creation".into());
                }
                return render(command, state);
            }
            "copy-url" | "copy-title" => {
                let content = if op == "copy-url" {
                    url
                } else {
                    text(&item, "title")
                };
                sdk::copy(ctx, content).await?;
                return render(command, state);
            }
            "reset" if kind == "bookmark" => {
                let url = url.to_owned();
                sdk::update_store(ctx, "chrome", move |s| {
                    if let Some(m) = s["bookmarks"].as_object_mut() {
                        m.remove(&url);
                    }
                    Ok(())
                })
                .await?;
            }
            "focus-tab" | "reload-tab" | "close-tab" if kind == "tab" => {
                browser::run(
                    json!({"kind":op,"window":item["window"],"tabId":item["tabId"],"url":url}),
                )
                .await?;
                hide(ctx).await?;
                return Ok(sdk::no_view("Chrome action completed"));
            }
            "focus-window" if kind == "window" => {
                browser::run(json!({"kind":op,"window":item["window"]})).await?;
                hide(ctx).await?;
                return Ok(sdk::no_view("Focused Chrome window"));
            }
            "open" | "guest" | "current" | "original" if kind != "window" => {
                if op == "open" && kind == "tab" {
                    return Err("Invalid Chrome action".into());
                }
                if op == "guest" {
                    browser::open_cli(&["--guest".into(), url.to_owned()]).await?;
                } else {
                    let behavior = match op {
                        "current" => "profile_current",
                        "original" => "profile_original",
                        _ => text(&state["preferences"], "openTabInProfile"),
                    };
                    browser::new_tab(
                        url,
                        behavior,
                        text(state, "profile"),
                        text(state, "profile"),
                    )
                    .await?;
                }
                if kind == "bookmark" {
                    let url = url.to_owned();
                    sdk::update_store(ctx, "chrome", move |s| {
                        let now = now_ms();
                        let previous = s["bookmarks"][&url]["count"].as_f64().unwrap_or(0.);
                        if !s["bookmarks"].is_object() {
                            s["bookmarks"] = json!({})
                        }
                        s["bookmarks"][&url] = json!({"count":previous+1.,"lastVisited":now});
                        Ok(())
                    })
                    .await?;
                }
                hide(ctx).await?;
                return Ok(sdk::no_view("Opened Chrome tab"));
            }
            _ => return Err("Unknown Chrome action".into()),
        }
    }
    state["limit"] = json!(100);
    load(ctx, command, state).await?;
    render(command, state)
}
async fn hide(ctx: &ExtensionContext) -> Result<(), String> {
    ctx.host("host.window.visibility", json!({"visible":false}))
        .await
        .map(|_| ())
}
impl Extension for Chrome {
    fn id(&self) -> &'static str {
        "com.tezbar.native.google-chrome"
    }
    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
    fn commands(&self) -> &'static [&'static str] {
        COMMANDS
    }
    fn handle<'a>(
        &'a self,
        ctx: &'a ExtensionContext,
        command: &'a str,
        operation: &'a str,
        p: &'a Value,
        state: &'a mut Value,
    ) -> BoxFuture<'a, CoreResult> {
        Box::pin(async move {
            if operation == "extension:dispose-session" {
                *state = json!({});
                return Ok(sdk::unchanged());
            }
            if !COMMANDS.contains(&command) {
                return Err("Unknown Chrome command".into());
            }
            if !ctx.platform.eq_ignore_ascii_case("macos") {
                return Err("This Google Chrome port supports macOS only, matching its upstream declaration".into());
            }
            match operation {
                "extension:run-command" => {
                    match command {
                        "new-window" | "new-incognito-window" | "name-window" => {
                            browser::run(json!({"kind":command})).await?;
                            hide(ctx).await?;
                            return Ok(sdk::no_view("Chrome action completed"));
                        }
                        "new-guest-window" => {
                            browser::open_cli(&["--guest".into()]).await?;
                            hide(ctx).await?;
                            return Ok(sdk::no_view("Opened Chrome guest window"));
                        }
                        "open-current-in-guest" => {
                            let url = browser::run(json!({"kind":"active-url"})).await?;
                            let url = url
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .ok_or("No active Chrome tab")?;
                            browser::open_cli(&["--guest".into(), url.into()]).await?;
                            hide(ctx).await?;
                            return Ok(sdk::no_view("Opened active tab in guest window"));
                        }
                        _ => {}
                    }
                    let store = sdk::read_store(ctx, "chrome").await?;
                    *state = json!({"preferences":p["preferences"],"query":p["searchText"].as_str().unwrap_or(""),"profile":store["profile"].as_str().unwrap_or("Default"),"limit":100,"revision":0});
                    load(ctx, command, state).await?;
                    render(command, state)
                }
                "extension:invoke-action" => invoke(ctx, command, p, state).await,
                "extension:search-text-changed" => {
                    state["query"] = json!(p["searchText"].as_str().unwrap_or(""));
                    state["limit"] = json!(100);
                    load(ctx, command, state).await?;
                    render(command, state)
                }
                "extension:refresh-session" => {
                    load(ctx, command, state).await?;
                    render(command, state)
                }
                "extension:load-more" => {
                    state["limit"] =
                        json!((state["limit"].as_u64().unwrap_or(100) + 100).min(10000));
                    render(command, state)
                }
                _ => Err("Unsupported Chrome operation".into()),
            }
        })
    }
}
#[tokio::main(worker_threads = 2)]
async fn main() {
    if let Err(e) = sdk::serve(Chrome).await {
        eprintln!("{e}");
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_only_views_ignore_malformed_profile_metadata() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("Local State"), "{").unwrap();
        for command in ["search-tab", "search-windows"] {
            let mut state = json!({"preferences":{},"profile":"Default"});
            assert_eq!(
                profile_directory(command, &mut state, || Ok(temp.path().to_owned())).unwrap(),
                None
            );
            assert_eq!(state["profiles"], json!([]));
            assert!(profile_directory(command, &mut state, || Err("No HOME".into())).is_ok());
        }
        let mut state = json!({"preferences":{},"profile":"Default"});
        assert!(
            profile_directory("search-history", &mut state, || Ok(temp.path().to_owned())).is_err()
        );
    }

    #[test]
    fn pagination_stops_at_cap_with_search_guidance() {
        for count in [10000usize, 10001] {
            let mut state = json!({"items":vec![json!({"kind":"window","title":"Fixture","window":1});count],
                "limit":10000,"profiles":[],"query":""});
            let view = render("search-windows", &mut state).unwrap();
            assert_eq!(view["hasMore"], false);
            let rows = view["root"]["children"].as_array().unwrap();
            assert_eq!(
                rows.iter().any(|row| row["props"]["id"] == "limit-reached"),
                count > 10000
            );
        }
    }
    #[tokio::test]
    async fn quicklink_create_and_cancel_use_only_explicit_host_dialog() {
        let dir = tempfile::tempdir().unwrap();
        let test = sdk::TestContext::new(
            sdk::ExtensionPaths {
                package_dir: dir.path().to_owned(),
                data_dir: dir.path().to_owned(),
                temp_dir: dir.path().to_owned(),
            },
            &["host.quicklinks.create"],
        );
        let mut state = json!({"items":[{"kind":"tab","title":"Synthetic Tab","url":"https://fixture.invalid","window":1,"index":1}],"profiles":[],"query":""});
        for status in ["cancelled", "created"] {
            let view = render("search-tab", &mut state).unwrap();
            let action = view["actions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|a| a["title"] == "Create Quick Link")
                .unwrap()["id"]
                .clone();
            test.reply(Ok(json!({"status":status})));
            test.run(invoke(
                &test.context,
                "search-tab",
                &json!({"actionId":action}),
                &mut state,
            ))
            .await
            .unwrap();
        }
        let calls = test.calls();
        assert_eq!(calls.len(), 2);
        for call in calls {
            assert_eq!(call["method"], "host.quicklinks.create");
            assert_eq!(
                call["params"],
                json!({"name":"Synthetic Tab","link":"https://fixture.invalid"})
            );
        }
    }
    #[test]
    fn stale_actions_cannot_target_replaced_rows() {
        let mut s = json!({"items":[{"kind":"tab","title":"one","url":"https://a","window":1,"index":1}],"revision":1,"profiles":[],"query":""});
        let a = render("search-tab", &mut s).unwrap();
        let first = s["revision"].as_u64().unwrap();
        assert!(a.to_string().contains(&format!("focus-tab:{first}:0")));
        let b = render("search-tab", &mut s).unwrap();
        let second = s["revision"].as_u64().unwrap();
        assert!(second > first);
        assert!(b.to_string().contains(&format!("focus-tab:{second}:0")));
    }
    #[test]
    fn ranking_decay_is_bounded() {
        assert_eq!(
            rank(&json!({"count":4,"lastVisited":0}), 72. * 3_600_000.),
            2.
        );
        assert_eq!(rank(&json!({"count":4,"lastVisited":1000}), 0.), 4.);
    }
    #[test]
    fn renders_profile_dropdown_and_complete_row_actions() {
        let mut s = json!({"items":[{"kind":"bookmark","title":"A","url":"https://a"}],"profiles":[{"id":"Default","name":"A"},{"id":"Profile 1","name":"B"}],"query":""});
        let a = render("search-bookmarks", &mut s).unwrap().to_string();
        for x in [
            "List.Dropdown",
            "Reset Ranking",
            "Open in Original Profile",
            "Copy URL",
        ] {
            assert!(a.contains(x))
        }
    }
}
