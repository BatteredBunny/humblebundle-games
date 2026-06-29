use crate::api::{Order, orders};
use crate::month::month_games;
use clap::{CommandFactory, Parser, ValueEnum};
use clap_complete::Shell;
use futures::future;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;

mod api;
mod cookies;
mod month;

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Default)]
enum OutputFormat {
    Json,
    Csv,

    #[default]
    Text,
}

/// Humble bundle keys
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// _simpleauth_sess cookie value. If omitted, Firefox cookies are used.
    #[arg(short, long)]
    token: Option<String>,

    /// format to output data in
    #[arg(short, long, default_value_t, value_enum)]
    format: OutputFormat,

    /// Print shell completions for the given shell to stdout and exit
    #[arg(long, value_enum, value_name = "SHELL")]
    completions: Option<Shell>,
}

fn print_completions(shell: Shell) {
    let mut command = Args::command();
    let name = command.get_name().to_string();
    clap_complete::generate(shell, &mut command, name, &mut std::io::stdout());
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

    if let Some(shell) = args.completions {
        print_completions(shell);
        return;
    }

    let token = args.token.clone().or_else(cookies::load).expect(
        "missing _simpleauth_sess cookie; pass --token or log in to humblebundle.com in Firefox",
    );

    let mut parsable_keys: Vec<ParsableFormat> = vec![];

    let keys = keys_page(token.clone()).await;
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
        future::join_all(chunks.map(|chunk| orders(token.clone(), chunk, &keys_progressbar)))
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
            && let Some(choice_url) = order.product.choice_url.clone()
        {
            let pb = ProgressBar::new_spinner();
            pb.enable_steady_tick(Duration::from_millis(120));
            pb.set_message(format!(
                "Searching for keys from {}",
                order.product.human_name
            ));
            let m = month_games(token.clone(), choice_url.clone()).await;
            pb.finish_and_clear();

            if order.choices_remaining > 0 || m.product_is_choiceless {
                let url = Some(format!(
                    "https://www.humblebundle.com/membership/{choice_url}",
                ));

                for id in m.into_tpkds() {
                    if id.is_valid() {
                        match args.format {
                            OutputFormat::Json | OutputFormat::Csv => {
                                parsable_keys.push(ParsableFormat {
                                    key: id.human_name.clone(),
                                    choice_url: url.clone(),
                                    platform: id.key_type.clone(),
                                })
                            }
                            OutputFormat::Text => {
                                keys_amount += 1;
                                id.display(keys_amount, url.clone());
                            }
                        }
                    }
                }

                continue;
            }
        }

        if let Some(tpkds) = order.tpkd_dict.get("all_tpks") {
            for id in tpkds {
                if id.is_valid() {
                    match args.format {
                        OutputFormat::Json | OutputFormat::Csv => {
                            parsable_keys.push(ParsableFormat {
                                key: id.human_name.clone(),
                                choice_url: None,
                                platform: id.key_type.clone(),
                            });
                        }
                        OutputFormat::Text => {
                            keys_amount += 1;
                            id.display(keys_amount, None);
                        }
                    }
                }
            }
        }
    }

    match args.format {
        OutputFormat::Json => {
            serde_json::to_writer(io::stdout(), &parsable_keys).unwrap();
        }
        OutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(io::stdout());

            for key in parsable_keys {
                wtr.serialize(key).unwrap();
            }

            wtr.flush().unwrap();
        }
        OutputFormat::Text => {
            println!("\n{keys_amount} unclaimed keys!")
        }
    }
}
