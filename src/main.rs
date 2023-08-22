use anyhow::{Context, Result};
use config;
use reqwest::Client;
use serde::{Deserialize, Serialize};

const BASE_URL: &str = "https://www.beeminder.com/api/v1/";

#[derive(Deserialize, Debug)]
struct Config {
    key: String,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct Goal {
    id: String,
    slug: String,
    updated_at: i64,
    title: String,
    roadall: Vec<(f64, Option<f64>, Option<f64>)>,
    delta: f64,
    headsum: String,
    limsum: String,
    safebuf: Option<i32>,
    safebump: Option<f64>,
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateGoal {
    roadall: Vec<(f64, Option<f64>, Option<f64>)>,
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

async fn add_datapoint(config: &Config, goal: &Goal, datapoint: &DataPoint) -> Result<()> {
    let client = Client::new();
    let url = format!(
        "{}/users/me/goals/{}/datapoints.json?auth_token={}",
        BASE_URL, goal.slug, config.key
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

impl UpdateGoal {
    pub fn from_goal(goal: &Goal) -> Self {
        UpdateGoal {
            roadall: goal.roadall.clone(),
        }
    }
}

async fn _update_goal(config: &Config, goal: &Goal) -> Result<()> {
    let client = Client::new();
    let url = format!(
        "{}/users/me/goals/{}.json?auth_token={}",
        BASE_URL, goal.slug, config.key
    );
    let update_goal = UpdateGoal::from_goal(goal);
    let update_json = serde_json::to_string(&update_goal)?;
    println!("Updating {}", goal.slug);
    let body = reqwest::Body::from(update_json);
    let resp = client
        .put(&url)
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await?;

    let body = resp.text().await?;
    println!("Response: {:?}", body);

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
    let url = format!("{}/users/me/goals.json?auth_token={}", BASE_URL, config.key);
    let goals: Vec<Goal> = client.get(&url).send().await?.json().await?;

    let to_update: Vec<(String, f64)> = vec![
        ("detoxify-sugar".to_string(), -20.0),
        ("detoxify-yt".to_string(), -3.0),
    ];

    for goal in goals {
        // println!("{} {} {}", goal.slug, goal.limsum, goal.delta);
        for (goal_slug, delta) in &to_update {
            if goal.slug == *goal_slug && goal.delta < *delta {
                println!("Ratchet {} to {}.", goal.slug, delta);
                let d = DataPoint {
                    value: delta - goal.delta,
                    comment: Some("ratchet".to_string()),
                    ..Default::default()
                };
                add_datapoint(&config, &goal, &d).await?;
            }
        }
    }

    Ok(())
}
