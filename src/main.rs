use chrono::{Local, TimeZone};
use anyhow::{Context, Result};
use std::fmt;
use colored::*;
use config;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;


const BASE_URL: &str = "https://www.beeminder.com/api/v1/";

#[derive(Deserialize, Debug)]
struct Config {
    key: String,
}

#[derive(StructOpt)]
enum Command {
    #[structopt(about = "List all goals")]
    List,
    #[structopt(about = "Add a datapoint")]
    Add {
        #[structopt(about = "The name of the goal")]
        goal: String,
        #[structopt(about = "The value of the datapoint")]
        value: f64,
        #[structopt(about = "An optional comment for the datapoint")]
        comment: Option<String>,
    },
}


#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Goal {
    id: String,
    slug: String,
    updated_at: i64,
    title: String,
    // roadall: Vec<(f64, Option<f64>, Option<f64>)>,
    delta: f64,
    headsum: String,
    limsum: String,
    pledge: f32,
    lastday: i64, 
    safebuf: i32,
    safebump: Option<f64>,
    tags: Vec<String>,
}

impl Goal {
    pub fn has_entry_today(&self) -> bool {
        let lastday_datetime = Local.timestamp_opt(self.lastday, 0).unwrap().date_naive();
        let today_datetime = Local::now().date_naive();
        lastday_datetime == today_datetime
    }
}

impl fmt::Display for Goal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let has_entry_today = if self.has_entry_today() { "✓" } else { "✗" };
        let slug_padded = format!("{:20}", self.slug);

        let color = match self.safebuf {
            0 => Color::Red,
            1 => Color::Yellow,
            2 => Color::Blue,
            3..=6 => Color::Green,
            _ => Color::White,
        };

        let colored_output = format!("{} {} [{}]", has_entry_today, slug_padded, self.limsum).color(color);

        write!(f, "{}", colored_output)
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DataPoint {
    value: f64,
    timestamp: Option<i64>,   // Defaults to "now" if none is passed in.
    daystamp: Option<String>, // Optional, timestamp takes precedence if both are included.
    comment: Option<String>,  // Optional.
    requestid: Option<String>, /* Optional unique id for datapoint, acts as an idempotency key.
                                  Used for verifying if Beeminder received a datapoint and
                                  preventing duplicates.*/
}

async fn add_datapoint(client: &Client, config: &Config, goal_slug: &str, datapoint: &DataPoint) -> Result<()> {
    let url = format!(
        "{}/users/me/goals/{}/datapoints.json?auth_token={}",
        BASE_URL, goal_slug, config.key
    );
    let update_json = serde_json::to_string(&datapoint)?;
    let body = reqwest::Body::from(update_json);
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;
    let _body = resp.text().await?;
    // println!("Response: {:?}", body);
    Ok(())
}


#[tokio::main]
async fn main() -> Result<()> {
    let settings = config::Config::builder()
        .add_source(config::File::with_name("settings"))
        .add_source(config::Environment::with_prefix("BEELINE"))
        .build()
        .with_context(|| format!("Could not load config."))?;

    let config = settings
        .try_deserialize::<Config>()
        .with_context(|| format!("Could not read config."))?;

    let client = Client::new();
    let command = Command::from_args();
    match command {
        Command::List => {
            let url = format!("{}/users/me/goals.json?auth_token={}", BASE_URL, config.key);
            let mut goals: Vec<Goal> = client.get(&url).send().await?.json().await?;

            goals.sort_by(|a, b| {
                let today_cmp = a.has_entry_today().cmp(&b.has_entry_today());
                if today_cmp != std::cmp::Ordering::Equal {
                    return today_cmp;
                }
                
                a.safebuf.cmp(&b.safebuf)
            });

            for goal in goals {
                println!("{}", goal);
            }
        }
        Command::Add { goal, value, comment } => {
            let datapoint = DataPoint {
                value,
                comment: comment.map(String::from),
                ..Default::default()
            };
            add_datapoint(&client, &config, &goal, &datapoint).await?;
        }
    }

    Ok(())
}
