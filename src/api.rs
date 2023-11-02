use indicatif::ProgressBar;
use serde::Deserialize;
use std::collections::HashMap;
use reqwest::Client;

#[derive(Deserialize, Debug, Clone)]
pub struct Order {
    pub product: OrderProduct,
    pub choices_remaining: u16,

    pub tpkd_dict: HashMap<String, Vec<AllTpks>>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct OrderProduct {
    pub category: String,
    pub human_name: String,
    pub choice_url: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct AllTpks {
    pub redeemed_key_val: Option<String>,
    pub human_name: String,
    pub is_expired: bool,
    pub key_type: String,
}

impl AllTpks {
    pub fn display(&self, num: u16, suffix: Option<String>) {
        print!("{num}. {} ({})", self.human_name, self.key_type);
        if let Some(suffix) = suffix {
            print!(" {suffix}")
        }
        println!()
    }

    pub fn is_valid(&self) -> bool {
        self.redeemed_key_val.is_none() && !self.is_expired
    }
}

pub async fn orders(token: String, ids: &[String], pb: &ProgressBar) -> Vec<Order> {
    let mut queries: Vec<(String, String)> = ids
        .iter()
        .map(|id| (String::from("gamekeys"), id.clone()))
        .collect();
    queries.push((String::from("all_tpkds"), String::from("true")));

    let body = Client::new()
        .get("https://www.humblebundle.com/api/v1/orders")
        .header("Cookie", format!("_simpleauth_sess={}", token))
        .query(&queries)
        .send()
        .await
        .expect("request to humble bundle failed")
        .text()
        .await
        .unwrap();

    let orders_map: HashMap<String, Order> = serde_json::from_str(&body).unwrap();
    pb.inc(1);
    orders_map.values().cloned().collect()
}