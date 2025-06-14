use anyhow::{Context, Result};
use beeminder::types::{CreateDatapoint, GoalSummary};
use beeminder::BeeminderClient;
use colored::{Color, Colorize};
use structopt::StructOpt;
use time::{OffsetDateTime, UtcOffset};
mod backup;
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
            edit::edit_datapoints(&client, &goal).await?;
        }
        Command::Backup { filename } => {
            backup::backup_user_data(&client, &filename).await?;
        }
    }

    Ok(())
}
