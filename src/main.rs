use anyhow::{Context, Result};
use beeminder::types::{CreateDatapoint, Datapoint, GoalSummary, UpdateDatapoint};
use beeminder::BeeminderClient;
use colored::{Color, Colorize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
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
}

#[derive(Debug)]
pub struct EditableDatapoint {
    pub id: Option<String>,
    pub timestamp: Option<OffsetDateTime>,
    pub value: Option<f64>,
    pub comment: Option<String>,
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
    }

    Ok(())
}
