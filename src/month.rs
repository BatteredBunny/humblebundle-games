
use serde::Deserialize;
use scraper::{Selector, Html};
use reqwest::Client;
use crate::AllTpks;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthPage {
    pub content_choice_options: MonthPageOptions,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthPageOptions {
    pub content_choice_data: MonthPageOptionsDataEnum,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum MonthPageOptionsDataEnum {
    Initial {
        #[serde(alias = "initial-get-all-games")]
        initial: MonthPageOptionsDataInitial,
    },

    GameData {
        game_data: HashMap<String, MonthPageOptionsDataGamesChoice>,
    },

    Unknown {},
}

#[derive(Deserialize)]
pub struct MonthPageOptionsDataInitial {
    pub content_choices: HashMap<String, MonthPageOptionsDataGamesChoice>,
}

#[derive(Deserialize)]
pub struct MonthPageOptionsDataGamesChoice {
    #[serde(flatten)]
    pub games: MonthPageOptionsDataGamesChoiceEnum,
}

#[derive(Deserialize, Debug, Clone)]
pub enum MonthPageOptionsDataGamesChoiceEnum {
    #[serde(rename = "tpkds")]
    Tpkds(Vec<AllTpks>),

    #[serde(rename = "nested_choice_tpkds")]
    NestedChoiceTpkds(HashMap<String, Vec<AllTpks>>),
}

pub async fn month_games(token: String, choice_url: String) -> MonthPage {
    let body = Client::new()
        .get(format!(
            "https://www.humblebundle.com/membership/{choice_url}"
        ))
        .header("Cookie", format!("_simpleauth_sess={}", token))
        .send()
        .await
        .expect("request to humble bundle failed")
        .text()
        .await
        .unwrap();

    let doc = Html::parse_document(&body);
    let selector = Selector::parse("#webpack-monthly-product-data").unwrap();

    let inner = doc
        .select(&selector)
        .next()
        .expect("couldnt find required info on page")
        .inner_html();
    serde_json::from_str(&inner).expect("failed to parse json for month page")
}