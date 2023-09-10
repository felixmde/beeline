use anyhow::{Context, Result};
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

    let command = Command::from_args();
    match command {
        Command::List => {
            for goal in goals {
                println!{"{:?}", goal};
                break;
                println!("{} {} {}", goal.slug, goal.limsum, goal.delta);
            }
        }
        Command::Add { goal, value, comment } => {
            // TODO: Implement adding a datapoint
        }
    }

    Ok(())
}
