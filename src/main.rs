use anyhow::{Context, Result};
use beeminder::types::{CreateDatapoint, Datapoint, GoalSummary, UpdateDatapoint};
use beeminder::BeeminderClient;
use colored::{Color, Colorize};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use std::process::Command as ProcessCommand;
use structopt::StructOpt;
use tempfile::NamedTempFile;
use time::{OffsetDateTime, UtcOffset};
mod edit;

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
    #[structopt(about = "Edit recent datapoints for a goal")]
    Edit {
        #[structopt(about = "The name of the goal")]
        goal: String,
    },
    #[structopt(about = "Backup all user data to JSON file")]
    Backup {
        #[structopt(about = "Output file name", default_value = "beedata.json")]
        filename: String,
    },
}

#[derive(Debug)]
pub struct EditableDatapoint {
    pub id: Option<String>,
    pub timestamp: Option<OffsetDateTime>,
    pub value: Option<f64>,
    pub comment: Option<String>,
}

#[derive(Serialize)]
struct BackupData {
    metadata: BackupMetadata,
    goals: Goals,
}

#[derive(Serialize)]
struct BackupMetadata {
    backup_timestamp: OffsetDateTime,
    beeline_version: String,
}

#[derive(Serialize)]
struct Goals {
    active: Vec<GoalWithDatapoints>,
    archived: Vec<GoalWithDatapoints>,
}

#[derive(Serialize)]
struct GoalWithDatapoints {
    goal: GoalSummary,
    datapoints: Vec<Datapoint>,
}

impl From<&Datapoint> for EditableDatapoint {
    fn from(dp: &Datapoint) -> Self {
        Self {
            id: Some(dp.id.clone()),
            timestamp: Some(dp.timestamp),
            value: Some(dp.value),
            comment: dp.comment.clone(),
        }
    }
}

fn has_entry_today(goal: &GoalSummary) -> bool {
    let now = OffsetDateTime::now_utc();
    let today_date = UtcOffset::current_local_offset()
        .map_or_else(|_| now, |offset| now.to_offset(offset))
        .date();
    goal.lastday.date() == today_date
}

fn format_goal(goal: &GoalSummary) -> String {
    let has_entry_today = if has_entry_today(goal) { "✓" } else { " " };
    let slug_padded = format!("{:20}", goal.slug);

    let color = match goal.safebuf {
        0 => Color::Red,
        1 => Color::Yellow,
        2 => Color::Blue,
        3..=6 => Color::Green,
        _ => Color::White,
    };

    format!("{} {} [{}]", has_entry_today, slug_padded, goal.limsum)
        .color(color)
        .to_string()
}

async fn edit_datapoints(client: &BeeminderClient, goal: &str) -> Result<()> {
    let datapoints = client
        .get_datapoints(goal, Some("timestamp"), Some(20))
        .await?;

    // Create temp file with datapoints and let user edit it
    let mut temp = NamedTempFile::new()?;
    edit::write_datapoints_tsv(&mut temp, &datapoints)?;
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    ProcessCommand::new(editor)
        .arg(temp.path())
        .status()
        .context("Failed to open editor")?;

    let reader = std::io::BufReader::new(File::open(temp.path())?);
    let edited_datapoints = edit::read_datapoints_tsv(reader)?;
    let orig_map: HashMap<String, &Datapoint> =
        datapoints.iter().map(|dp| (dp.id.clone(), dp)).collect();
    let mut ids_to_delete: HashSet<String> = datapoints.iter().map(|dp| dp.id.clone()).collect();

    for dp in edited_datapoints {
        match dp {
            EditableDatapoint { id: Some(id), .. } => {
                if let Some(orig) = orig_map.get(&id) {
                    ids_to_delete.remove(&id);
                    let needs_update = dp.value != Some(orig.value)
                        || dp.timestamp != Some(orig.timestamp)
                        || dp.comment != orig.comment;
                    if needs_update {
                        let update = UpdateDatapoint {
                            id: id.clone(),
                            timestamp: dp.timestamp,
                            value: dp.value,
                            comment: dp.comment,
                        };
                        println!("Updating datapoint '{id}'.");
                        client.update_datapoint(goal, &update).await?;
                    }
                } else {
                    eprintln!("No datapoint with ID '{id}'.");
                }
            }
            EditableDatapoint { id: None, .. } => {
                let create = CreateDatapoint {
                    timestamp: dp.timestamp,
                    value: dp.value.unwrap_or_default(),
                    comment: dp.comment,
                    daystamp: None,
                    requestid: None,
                };
                println!(
                    "Creating new datapoint with value '{}'.",
                    dp.value.unwrap_or_default()
                );
                client.create_datapoint(goal, &create).await?;
            }
        }
    }

    for id in ids_to_delete {
        println!("Deleting datapoint '{id}'.");
        client.delete_datapoint(goal, &id).await?;
    }

    Ok(())
}

async fn backup_user_data(client: &BeeminderClient, filename: &str) -> Result<()> {
    println!("Starting backup...");

    println!("Fetching active goals...");
    let active_goals = client
        .get_goals()
        .await
        .with_context(|| "Failed to fetch active goals")?;

    println!("Fetching archived goals...");
    let archived_goals = client
        .get_archived_goals()
        .await
        .with_context(|| "Failed to fetch archived goals")?;

    let total_goals = active_goals.len() + archived_goals.len();
    println!(
        "Found {} active goals and {} archived goals",
        active_goals.len(),
        archived_goals.len()
    );

    let mut active_goals_with_data = Vec::new();
    let mut archived_goals_with_data = Vec::new();
    let mut processed = 0;

    for goal in active_goals {
        processed += 1;
        println!(
            "Fetching datapoints for active goal: {} ({}/{})",
            goal.slug, processed, total_goals
        );
        let datapoints = client
            .get_datapoints(&goal.slug, Some("timestamp"), None)
            .await
            .with_context(|| {
                format!("Failed to fetch datapoints for active goal: {}", goal.slug)
            })?;
        println!("  Found {} datapoints", datapoints.len());
        active_goals_with_data.push(GoalWithDatapoints { goal, datapoints });
    }

    for goal in archived_goals {
        processed += 1;
        println!(
            "Fetching datapoints for archived goal: {} ({}/{})",
            goal.slug, processed, total_goals
        );
        let datapoints = client
            .get_datapoints(&goal.slug, Some("timestamp"), None)
            .await
            .with_context(|| {
                format!(
                    "Failed to fetch datapoints for archived goal: {}",
                    goal.slug
                )
            })?;
        println!("  Found {} datapoints", datapoints.len());
        archived_goals_with_data.push(GoalWithDatapoints { goal, datapoints });
    }

    let backup_data = BackupData {
        metadata: BackupMetadata {
            backup_timestamp: OffsetDateTime::now_utc(),
            beeline_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        goals: Goals {
            active: active_goals_with_data,
            archived: archived_goals_with_data,
        },
    };

    println!("Writing backup to file: {}", filename);
    let json_data = serde_json::to_string_pretty(&backup_data)
        .with_context(|| "Failed to serialize backup data to JSON")?;
    let mut file = File::create(filename)
        .with_context(|| format!("Failed to create backup file: {}", filename))?;
    file.write_all(json_data.as_bytes())
        .with_context(|| format!("Failed to write backup data to file: {}", filename))?;

    println!("Backup completed successfully! Saved to: {}", filename);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let api_key = std::env::var("BEEMINDER_API_KEY")
        .with_context(|| "Please create environment variable BEEMINDER_API_KEY".to_string())?;

    let client = BeeminderClient::new(api_key);
    let command = Command::from_args();
    match command {
        Command::List => {
            let mut goals: Vec<GoalSummary> = client.get_goals().await?;

            goals.sort_by(|a, b| {
                let today_cmp = has_entry_today(a).cmp(&has_entry_today(b));
                if today_cmp != std::cmp::Ordering::Equal {
                    return today_cmp;
                }

                a.safebuf.cmp(&b.safebuf)
            });

            for goal in goals {
                println!("{}", format_goal(&goal));
            }
        }
        Command::Add {
            goal,
            value,
            comment,
        } => {
            let mut dp = CreateDatapoint::new(value);
            if let Some(comment) = comment {
                dp = dp.with_comment(&comment);
            }
            client.create_datapoint(&goal, &dp).await?;
        }
        Command::Edit { goal } => {
            edit_datapoints(&client, &goal).await?;
        }
        Command::Backup { filename } => {
            backup_user_data(&client, &filename).await?;
        }
    }

    Ok(())
}
