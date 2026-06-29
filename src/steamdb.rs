use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

pub struct SteamDB {
    pub id: String,
    pub url: String,
    pub api_key: String,
}

#[derive(Serialize)]
pub struct SearchBody {
    requests: Vec<SearchBodyInner>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchBodyInner {
    index_name: String,
    params: String,
}

#[derive(Deserialize)]
pub struct SearchOutput {
    results: Vec<SearchOutputInner>,
}

#[derive(Deserialize)]
pub struct SearchOutputInner {
    hits: Vec<SearchOutputInnerHit>,
}

#[derive(Deserialize)]
pub struct SearchOutputInnerHit {
    price_us: f64,

    #[serde(rename = "userScore")]
    user_score: Option<f64>,

    #[serde(rename = "objectID")]
    object_id: String,
}

#[derive(Serialize, Debug)]
pub struct SearchResult {
    pub url: String,
    pub user_score: Option<f64>,
    pub price_us: f64,
}

impl SteamDB {
    pub async fn new() -> Option<SteamDB> {
        if let Some((id, key)) = Self::legacy_credentials().await {
            return Some(Self::from_credentials(id, key));
        }

        if let Some((id, key)) = Self::search_page_credentials().await {
            return Some(Self::from_credentials(id, key));
        }

        eprintln!("warning: could not discover SteamDB search credentials; skipping SteamDB data");
        None
    }

    fn from_credentials(id: String, api_key: String) -> SteamDB {
        SteamDB {
            url: format!("https://{id}-dsn.algolia.net/1/indexes/*/queries"),
            id,
            api_key,
        }
    }

    async fn legacy_credentials() -> Option<(String, String)> {
        let re = Regex::new(r#"algoliasearch\("([^"]+)","([^"]+)"\)"#).unwrap();
        let body = reqwest::get("https://steamdb.info/static/js/instantsearch.js")
            .await
            .ok()?
            .text()
            .await
            .ok()?;
        let captures = re.captures(&body)?;
        Some((
            captures.get(1)?.as_str().to_string(),
            captures.get(2)?.as_str().to_string(),
        ))
    }

    async fn search_page_credentials() -> Option<(String, String)> {
        let id_re = Regex::new(r#"data-a="([^"]+)""#).unwrap();
        let key_re = Regex::new(r#"data-k="([^"]+)""#).unwrap();
        let body = reqwest::get("https://steamdb.info/search/?a=app")
            .await
            .ok()?
            .text()
            .await
            .ok()?;
        Some((
            id_re.captures(&body)?.get(1)?.as_str().to_string(),
            key_re.captures(&body)?.get(1)?.as_str().to_string(),
        ))
    }

    pub async fn search(&self, query: &str) -> SearchResult {
        let mut url = Url::parse("https://example.net").unwrap();
        url.query_pairs_mut()
            .clear()
            .append_pair("hitsPerPage", "40")
            .append_pair("page", "0")
            .append_pair("query", query)
            .append_pair("maxValuesPerFacet", "200")
            .append_pair("attributesToRetrieve", "[\"price_us\",\"userScore\"]");

        let b = SearchBody {
            requests: vec![SearchBodyInner {
                index_name: "steamdb".to_string(),
                params: url.query().unwrap().to_string(),
            }],
        };

        let j = serde_json::to_string(&b).unwrap();

        let body = reqwest::Client::new()
            .post(self.url.clone())
            .header("Referer", "https://steamdb.info/")
            .query(&[
                ("x-algolia-application-id", self.id.clone()),
                ("x-algolia-api-key", self.api_key.clone()),
            ])
            .body(j)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        let s: SearchOutput = serde_json::from_str(&body).expect("failed to parse search json");
        let res = &s.results[0].hits[0];

        SearchResult {
            url: format!("https://steamdb.info/app/{}/", res.object_id),
            user_score: res.user_score,
            price_us: res.price_us,
        }
    }
}
