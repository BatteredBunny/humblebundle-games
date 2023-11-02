#![feature(let_chains)]

use crate::api::{orders, AllTpks, Order};
use crate::month::{month_games, MonthPageOptionsDataEnum, MonthPageOptionsDataGamesChoiceEnum};
use clap::Parser;
use futures::future;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::time::Duration;

mod api;
mod month;

/// Humble bundle keys
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// _simpleauth_sess cookie value
    #[arg(short, long)]
    token: String,

    /// return data in json
    #[arg(short, long)]
    json: bool,
}

#[derive(Deserialize, Debug)]
struct KeysPage {
    gamekeys: Vec<String>,
}

async fn keys_page(token: String) -> KeysPage {
    let body = Client::new()
        .get("https://www.humblebundle.com/home/keys")
        .header("Cookie", format!("_simpleauth_sess={}", token))
        .send()
        .await
        .expect("request to humble bundle failed")
        .text()
        .await
        .unwrap();

    let doc = Html::parse_document(&body);
    let selector = Selector::parse("#user-home-json-data").unwrap();

    let inner = doc
        .select(&selector)
        .next()
        .expect("couldnt find required info on page")
        .inner_html();
    serde_json::from_str(&inner).expect("failed to parse json for keys page")
}

#[derive(Serialize, Debug)]
struct ParsableFormat {
    key: String,
    choice_url: Option<String>,
    platform: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let mut parsable_keys: Vec<ParsableFormat> = vec![];

    let keys = keys_page(args.token.clone()).await;
    let chunks = keys.gamekeys.chunks(40);
    let keys_progressbar = ProgressBar::new(chunks.len() as u64);
    keys_progressbar.set_style(
        ProgressStyle::with_template("{prefix:.bold.dim}[{pos}/{len}] {spinner} {wide_msg}")
            .unwrap()
            .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈ "),
    );
    keys_progressbar.enable_steady_tick(Duration::from_millis(120));
    keys_progressbar.set_message("Searching for keys");

    let total_orders: Vec<Order> =
        future::join_all(chunks.map(|chunk| orders(args.token.clone(), chunk, &keys_progressbar)))
            .await
            .into_par_iter()
            .reduce_with(|mut acc, e| {
                acc.extend(e.iter().cloned());
                acc
            })
            .unwrap();

    keys_progressbar.set_message("Found all keys!\n");
    keys_progressbar.finish_and_clear();

    let mut keys_amount = 0;
    for order in total_orders {
        if order.product.category == "subscriptioncontent"
            && order.choices_remaining > 0
            && let Some(choice_url) = order.product.choice_url
        {
            let pb = ProgressBar::new_spinner();
            pb.enable_steady_tick(Duration::from_millis(120));
            pb.set_message(format!(
                "Searching for keys from {}",
                order.product.human_name
            ));
            let m = month_games(args.token.clone(), choice_url.clone()).await;
            pb.finish_and_clear();

            for (_, choice) in match m.content_choice_options.content_choice_data {
                MonthPageOptionsDataEnum::Initial { initial } => {
                    initial.content_choices.into_iter()
                }
                MonthPageOptionsDataEnum::GameData { game_data } => game_data.into_iter(),
                MonthPageOptionsDataEnum::Unknown {} => continue,
            } {
                for id in match choice.games {
                    MonthPageOptionsDataGamesChoiceEnum::Tpkds(t) => t,
                    MonthPageOptionsDataGamesChoiceEnum::NestedChoiceTpkds(t) => t
                        .values()
                        .cloned()
                        .reduce(|mut acc, e| {
                            acc.extend(e.iter().cloned());
                            acc
                        })
                        .unwrap(),
                } {
                    if id.is_valid() {
                        let url = Some(format!(
                            "https://www.humblebundle.com/membership/{choice_url}",
                        ));

                        if !args.json {
                            keys_amount += 1;
                            id.display(keys_amount, url);
                        } else {
                            parsable_keys.push(ParsableFormat {
                                key: id.human_name.clone(),
                                choice_url: url,
                                platform: id.key_type.clone(),
                            })
                        }
                    }
                }
            }
        } else {
            for id in order.tpkd_dict["all_tpks"].iter() {
                if id.is_valid() {
                    if !args.json {
                        keys_amount += 1;
                        id.display(keys_amount, None);
                    } else {
                        parsable_keys.push(ParsableFormat {
                            key: id.human_name.clone(),
                            choice_url: None,
                            platform: id.key_type.clone(),
                        })
                    }
                }
            }
        }
    }

    if !args.json {
        println!("\n{keys_amount} unclaimed keys!")
    } else if let Ok(j) = serde_json::to_string(&parsable_keys) {
        println!("{j}")
    }
}
