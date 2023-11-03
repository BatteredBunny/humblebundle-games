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
    pub async fn new() -> SteamDB {
        let re = Regex::new(r#"t=algoliasearch\("(.+)","(.+)"\);"#).unwrap();

        let body = reqwest::get("https://steamdb.info/static/js/instantsearch.js")
            .await
            .unwrap()
            .text()
            .await
            .unwrap();

        let m = re.captures(&body).unwrap();

        let id = m.get(1).unwrap().as_str();
        let key = m.get(2).unwrap().as_str();

        SteamDB {
            id: id.to_string(),
            url: format!("https://{id}-dsn.algolia.net/1/indexes/*/queries"),
            api_key: key.to_string(),
        }
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
