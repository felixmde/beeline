use anyhow::{Context, Result};
use beeminder::types::{Datapoint, GoalSummary};
use beeminder::BeeminderClient;
use serde::Serialize;
use std::fs::File;
use std::io::Write;
use time::OffsetDateTime;

#[derive(Serialize)]
pub struct BackupData {
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

pub async fn backup_user_data(client: &BeeminderClient, filename: &str) -> Result<()> {
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

    println!("Writing backup to file: {filename}");
    let json_data = serde_json::to_string_pretty(&backup_data)
        .with_context(|| "Failed to serialize backup data to JSON")?;
    let mut file = File::create(filename)
        .with_context(|| format!("Failed to create backup file: {filename}"))?;
    file.write_all(json_data.as_bytes())
        .with_context(|| format!("Failed to write backup data to file: {filename}"))?;

    println!("Backup completed successfully! Saved to: {filename}");
    Ok(())
}
