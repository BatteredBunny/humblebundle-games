use futures::{StreamExt, stream};
use reqwest::header::{ACCEPT, COOKIE, HeaderMap, HeaderValue};
use reqwest::multipart::Form;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::error::Error;

pub type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

pub const BASE_URL: &str = "https://www.humblebundle.com";
const LIBRARY_ACTION: &str = "filterUserLibrary";
const PAGE_LIMIT: u32 = 500;
const CONCURRENCY: usize = 8;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LibraryPage {
    #[serde(default)]
    pub results: Vec<LibraryItem>,
    pub pagination: Pagination,
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub total_pages: u32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub sub_title: Option<String>,
    pub item_type: String,
    #[serde(default)]
    pub entitlements: Entitlements,
}

#[derive(Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Entitlements {
    #[serde(default)]
    pub keys: Vec<Key>,
    #[serde(default)]
    pub redemption_groups: Vec<RedemptionGroup>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Key {
    pub id: String,
    pub title: String,
    #[serde(rename = "type")]
    pub platform: String,
    #[serde(default)]
    pub redemption_data: Option<RedemptionData>,
    #[serde(default)]
    pub redeemed_at: Option<String>,
    #[serde(default)]
    pub expired: bool,
    #[serde(default)]
    pub gift_status: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionData {
    #[serde(default)]
    pub key_formatted: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionGroup {
    pub max_redemptions: u32,
    #[serde(default)]
    pub options: Vec<RedemptionOption>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionOption {
    pub entitlement_id: String,
    #[serde(default)]
    pub eligible: bool,
    #[serde(default)]
    pub redeemed: bool,
}

impl Key {
    pub fn is_unclaimed(&self) -> bool {
        self.redeemed_at.is_none()
            && self
                .redemption_data
                .as_ref()
                .is_none_or(|data| data.key_formatted.is_none())
            && !self.expired
            && self.gift_status.as_deref() != Some("gifted")
    }
}

impl LibraryItem {
    pub fn is_group(&self) -> bool {
        self.item_type == "entitlement-group"
    }

    pub fn url(&self) -> String {
        let kind = if self.is_group() { "group" } else { "content" };
        format!("{BASE_URL}/home/library/{kind}/{}", self.id)
    }

    /// Keys that can still be revealed. For Choice months this respects how
    /// many picks are left and which games are still eligible.
    pub fn unclaimed_keys(&self) -> Vec<&Key> {
        let groups = &self.entitlements.redemption_groups;
        let choosable = |key: &Key| {
            groups.iter().all(|group| {
                let redeemed = group.options.iter().filter(|o| o.redeemed).count() as u32;
                let remaining = group.max_redemptions.saturating_sub(redeemed);
                match group.options.iter().find(|o| o.entitlement_id == key.id) {
                    Some(option) => remaining > 0 && option.eligible && !option.redeemed,
                    None => true,
                }
            })
        };

        self.entitlements
            .keys
            .iter()
            .filter(|key| key.is_unclaimed() && choosable(key))
            .collect()
    }
}

pub struct Humble {
    client: reqwest::Client,
}

impl Humble {
    /// `cookies` is a `Cookie` header value (`a=1; b=2`); the site authenticates
    /// with an `ory_session_*` cookie.
    pub fn new(cookies: &str) -> Result<Self> {
        if !cookies.contains("ory_session_") {
            return Err("cookies don't contain an ory_session_* login cookie; \
                        log in to humblebundle.com again"
                .into());
        }
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str(cookies)?);
        let client = reqwest::Client::builder()
            .user_agent(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
            .default_headers(headers)
            .build()?;
        Ok(Self { client })
    }

    async fn get_text(&self, url: &str, rsc: bool) -> Result<String> {
        let mut request = self.client.get(url);
        if rsc {
            request = request.header("RSC", "1");
        }
        let response = request.send().await?.error_for_status()?;
        if response.url().path().contains("login") {
            return Err("not logged in: humble bundle redirected to the login page, \
                        your session cookie is probably expired"
                .into());
        }
        Ok(response.text().await?)
    }

    /// Finds the id of the `filterUserLibrary` server action in the library page's JS chunks.
    pub async fn library_action_id(&self) -> Result<String> {
        let html = self.get_text(&format!("{BASE_URL}/home/library"), false).await?;
        let chunks = script_sources(&html);

        let found = stream::iter(chunks)
            .map(|src| async move {
                let url = if src.starts_with('/') {
                    format!("{BASE_URL}{src}")
                } else {
                    src
                };
                self.get_text(&url, false).await.ok()
            })
            .buffer_unordered(CONCURRENCY)
            .filter_map(|js| async move { find_server_action_id(&js?, LIBRARY_ACTION) });

        std::pin::pin!(found).next().await.ok_or_else(|| {
            format!("couldn't find the {LIBRARY_ACTION} server action, the website changed again")
                .into()
        })
    }

    async fn library_page(&self, action_id: &str, page: u32) -> Result<LibraryPage> {
        let form = Form::new()
            .text("_1_requestId", "1")
            .text("_1_query", "")
            .text("_1_sort", "mostRecent")
            .text("_1_cursor", ((page - 1) * PAGE_LIMIT).to_string())
            .text("_1_page", page.to_string())
            .text("_1_limit", PAGE_LIMIT.to_string())
            .text("_1_hideRevealed", "true")
            .text("_1_hideExpired", "true")
            .text("_1_dateExpires", "")
            .text("0", r#"["$undefined","$K1"]"#);

        let body = self
            .client
            .post(format!("{BASE_URL}/home/library"))
            .header("Next-Action", action_id)
            .header(ACCEPT, "text/x-component")
            .multipart(form)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;

        let page = parse_library_page(&body).ok_or("failed to parse library response")?;
        if !page.success {
            let error = page.error.as_deref().unwrap_or("unknown error");
            return Err(match error {
                "not_authenticated" => "humble bundle says you're not logged in \
                                        (not_authenticated); check your session cookie"
                    .into(),
                _ => format!("humble bundle returned an error: {error}").into(),
            });
        }
        Ok(page)
    }

    /// All library items that still have unrevealed content.
    pub async fn library(&self, action_id: &str) -> Result<Vec<LibraryItem>> {
        let first = self.library_page(action_id, 1).await?;
        let mut items = first.results;

        let rest: Vec<Result<LibraryPage>> = stream::iter(2..=first.pagination.total_pages)
            .map(|page| self.library_page(action_id, page))
            .buffered(CONCURRENCY)
            .collect()
            .await;
        for page in rest {
            items.extend(page?.results);
        }

        Ok(items)
    }

    /// Full details of an `entitlement-group` (e.g. a Choice month with picks left).
    pub async fn group(&self, id: &str) -> Result<LibraryItem> {
        let body = self
            .get_text(&format!("{BASE_URL}/home/library/group/{id}"), true)
            .await?;
        parse_group_page(&body).ok_or_else(|| format!("failed to parse library group {id}").into())
    }
}

fn script_sources(html: &str) -> Vec<String> {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("script[src]").unwrap();
    let mut sources: Vec<String> = doc
        .select(&selector)
        .filter_map(|el| el.value().attr("src"))
        .filter(|src| src.contains("/_next/static/chunks/"))
        .map(String::from)
        .collect();
    sources.sort_unstable();
    sources.dedup();
    sources
}

/// Extracts the id from `createServerReference)("<id>",…,"<name>")` in minified JS.
fn find_server_action_id(js: &str, name: &str) -> Option<String> {
    let needle = format!(",\"{name}\")");
    let end = js.find(&needle)?;
    let marker = "createServerReference)(\"";
    let start = js[..end].rfind(marker)? + marker.len();
    let id: String = js[start..]
        .chars()
        .take_while(char::is_ascii_hexdigit)
        .collect();
    (id.len() >= 40).then_some(id)
}

/// Server action responses are React flight rows (`<id>:<json>`); find the one with results.
fn parse_library_page(body: &str) -> Option<LibraryPage> {
    body.lines()
        .filter_map(|line| line.split_once(':'))
        .find_map(|(_, json)| serde_json::from_str(json).ok())
}

/// The group page's RSC payload embeds `{"userLibraryItem":{…}}` as props of a client component.
fn parse_group_page(body: &str) -> Option<LibraryItem> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Props {
        user_library_item: LibraryItem,
    }

    let start = body.find("{\"userLibraryItem\":")?;
    let json = balanced_object(&body[start..])?;
    serde_json::from_str::<Props>(json)
        .ok()
        .map(|props| props.user_library_item)
}

/// Returns the JSON object at the start of `s`, matched by braces (string-aware).
fn balanced_object(s: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (i, c) in s.char_indices() {
        if in_string {
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&s[..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn key(id: &str, title: &str) -> serde_json::Value {
        json!({
            "id": id,
            "title": title,
            "subTitle": "January 2022 Humble Choice",
            "type": "steam",
            "redemptionData": {"keyFormatted": null, "externalOrderStatus": null},
            "redeemedAt": null,
            "expired": false,
            "giftStatus": "unassigned",
            "redemptionExpiryDate": "$undefined"
        })
    }

    #[test]
    fn finds_server_action_id() {
        let id = "40d9c0e5b0f4a3c2e1d6b7a8f9e0d1c2b3a4f5e6a7";
        let js = format!(
            r#"a=(0,b.createServerReference)("00aa",b.callServer,void 0,b.findSourceMapURL,"other"),c=(0,b.createServerReference)("{id}",b.callServer,void 0,b.findSourceMapURL,"filterUserLibrary")"#
        );
        assert_eq!(find_server_action_id(&js, "filterUserLibrary").as_deref(), Some(id));
        assert_eq!(find_server_action_id(&js, "missing"), None);
    }

    #[test]
    fn parses_library_action_response() {
        let row = json!({
            "results": [{
                "id": "a",
                "title": "Space Haven",
                "subTitle": "November 2026 Humble Choice",
                "itemType": "content-entitlement",
                "type": "steam",
                "preOrder": "$undefined",
                "entitlements": {"keys": [key("a", "Space Haven")], "downloads": []}
            }],
            "bundlesCount": 0,
            "pagination": {"page": 1, "totalPages": 1, "totalDocs": 1},
            "success": true
        });
        let body = format!("0:{{\"a\":\"$@1\",\"q\":\"\"}}\n1:{row}\n");

        let page = parse_library_page(&body).unwrap();
        assert_eq!(page.pagination.total_pages, 1);
        assert_eq!(page.results[0].unclaimed_keys()[0].title, "Space Haven");
    }

    #[test]
    fn skips_revealed_expired_and_gifted_keys() {
        let mut revealed = key("1", "Revealed");
        revealed["redemptionData"]["keyFormatted"] = json!("XXXXX-XXXXX-XXXXX");
        let mut expired = key("2", "Expired");
        expired["expired"] = json!(true);
        let mut gifted = key("3", "Gifted");
        gifted["giftStatus"] = json!("gifted");

        let item: LibraryItem = serde_json::from_value(json!({
            "id": "x",
            "title": "Bundle",
            "itemType": "content-entitlement",
            "entitlements": {"keys": [revealed, expired, gifted, key("4", "Fine")]}
        }))
        .unwrap();

        let titles: Vec<_> = item.unclaimed_keys().iter().map(|k| &k.title).collect();
        assert_eq!(titles, ["Fine"]);
    }

    #[test]
    fn parses_choice_group_and_respects_remaining_picks() {
        let mut chosen = key("k1", "Chosen");
        chosen["redeemedAt"] = json!("2022-01-10T00:00:00.000Z");
        let item = json!({
            "id": "g",
            "title": "January 2022 Humble Choice",
            "itemType": "entitlement-group",
            "type": "",
            "entitlements": {
                "keys": [chosen, key("k2", "Pickable"), key("k3", "Ineligible")],
                "downloads": [],
                "redemptionGroups": [{
                    "groupId": "g",
                    "maxRedemptions": 2,
                    "state": "MULTI",
                    "options": [
                        {"entitlementId": "k1", "eligible": false, "redeemed": true},
                        {"entitlementId": "k2", "eligible": true, "redeemed": false},
                        {"entitlementId": "k3", "eligible": false, "redeemed": false}
                    ]
                }]
            }
        });
        let body = format!(
            r#"5:["$","div",null,{{"children":[["$","$L79",null,{{"userLibraryItem":{item},"showDisclaimer":false}}]]}}]"#
        );

        let group = parse_group_page(&body).unwrap();
        let titles: Vec<_> = group.unclaimed_keys().iter().map(|k| &k.title).collect();
        assert_eq!(titles, ["Pickable"]);

        let mut full = group.clone();
        full.entitlements.redemption_groups[0].max_redemptions = 1;
        assert!(full.unclaimed_keys().is_empty());
    }

    #[test]
    fn balanced_object_ignores_braces_in_strings() {
        let s = r#"{"a":"}{\"","b":{"c":1}} trailing"#;
        assert_eq!(balanced_object(s), Some(r#"{"a":"}{\"","b":{"c":1}}"#));
    }
}
