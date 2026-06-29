use crate::api::AllTpks;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthPage {
    #[serde(default)]
    pub product_is_choiceless: bool,
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

    GameData(MonthPageOptionsDataGameData),

    Unknown {},
}

#[derive(Deserialize)]
pub struct MonthPageOptionsDataInitial {
    pub content_choices: HashMap<String, MonthPageOptionsDataGamesChoice>,
}

#[derive(Deserialize)]
pub struct MonthPageOptionsDataGameData {
    #[serde(default)]
    pub display_order: Vec<String>,
    pub game_data: HashMap<String, MonthPageOptionsDataGamesChoice>,
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

impl MonthPage {
    pub fn into_tpkds(self) -> Vec<AllTpks> {
        self.content_choice_options.content_choice_data.into_tpkds()
    }
}

impl MonthPageOptionsDataEnum {
    fn into_tpkds(self) -> Vec<AllTpks> {
        match self {
            Self::Initial { initial } => initial
                .content_choices
                .into_values()
                .flat_map(MonthPageOptionsDataGamesChoice::into_tpkds)
                .collect(),
            Self::GameData(game_data) => game_data.into_tpkds(),
            Self::Unknown {} => Vec::new(),
        }
    }
}

impl MonthPageOptionsDataGameData {
    fn into_tpkds(mut self) -> Vec<AllTpks> {
        let mut tpkds = Vec::new();

        for key in self.display_order {
            if let Some(choice) = self.game_data.remove(&key) {
                tpkds.extend(choice.into_tpkds());
            }
        }

        tpkds.extend(
            self.game_data
                .into_values()
                .flat_map(MonthPageOptionsDataGamesChoice::into_tpkds),
        );
        tpkds
    }
}

impl MonthPageOptionsDataGamesChoice {
    fn into_tpkds(self) -> Vec<AllTpks> {
        match self.games {
            MonthPageOptionsDataGamesChoiceEnum::Tpkds(tpkds) => tpkds,
            MonthPageOptionsDataGamesChoiceEnum::NestedChoiceTpkds(tpkds) => {
                tpkds.into_values().flatten().collect()
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_choiceless_game_data_without_redeemed_key_field() {
        let page: MonthPage = serde_json::from_value(json!({
            "productIsChoiceless": true,
            "contentChoiceOptions": {
                "contentChoiceData": {
                    "display_order": ["diabloiv", "othergame"],
                    "extras": [],
                    "game_data": {
                        "othergame": {
                            "tpkds": [{
                                "human_name": "Other Game",
                                "is_expired": false,
                                "key_type": "steam"
                            }]
                        },
                        "diabloiv": {
                            "tpkds": [{
                                "human_name": "Diablo IV",
                                "is_expired": false,
                                "key_type": "battlenet"
                            }]
                        }
                    }
                }
            }
        }))
        .unwrap();

        assert!(page.product_is_choiceless);

        let tpkds = page.into_tpkds();

        assert_eq!(tpkds[0].human_name, "Diablo IV");
        assert_eq!(tpkds[0].key_type, "battlenet");
        assert!(tpkds[0].is_valid());
    }

    #[test]
    fn flattens_nested_choice_tpkds() {
        let page: MonthPage = serde_json::from_value(json!({
            "contentChoiceOptions": {
                "contentChoiceData": {
                    "initial-get-all-games": {
                        "content_choices": {
                            "choice": {
                                "nested_choice_tpkds": {
                                    "steam": [{
                                        "human_name": "Steam Game",
                                        "redeemed_key_val": null,
                                        "is_expired": false,
                                        "key_type": "steam"
                                    }],
                                    "gog": [{
                                        "human_name": "GOG Game",
                                        "redeemed_key_val": null,
                                        "is_expired": false,
                                        "key_type": "gog"
                                    }]
                                }
                            }
                        }
                    }
                }
            }
        }))
        .unwrap();

        let mut names: Vec<_> = page
            .into_tpkds()
            .into_iter()
            .map(|tpkd| tpkd.human_name)
            .collect();
        names.sort();

        assert_eq!(names, ["GOG Game", "Steam Game"]);
    }
}
