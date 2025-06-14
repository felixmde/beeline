use crate::EditableDatapoint;
use anyhow::{Context, Result};
use beeminder::types::{CreateDatapoint, Datapoint, UpdateDatapoint};
use beeminder::BeeminderClient;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, Write};
use std::process::Command as ProcessCommand;
use tempfile::NamedTempFile;
use time::macros::format_description;
use time::{PrimitiveDateTime, UtcOffset};

const TIMESTAMP_FORMAT: &[time::format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

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

pub fn write_datapoints_tsv(writer: &mut impl Write, datapoints: &Vec<Datapoint>) -> Result<()> {
    writeln!(writer, "TIMESTAMP\tVALUE\tCOMMENT\tID")?;
    let offset = UtcOffset::current_local_offset()?;

    for dp in datapoints {
        let time = dp.timestamp.to_offset(offset);
        let timestamp = time.format(TIMESTAMP_FORMAT)?;
        let comment = dp.comment.as_deref().unwrap_or("");
        writeln!(
            writer,
            "{}\t{}\t{}\t{}",
            timestamp, dp.value, comment, dp.id
        )?;
    }
    Ok(())
}

pub fn read_datapoints_tsv(reader: impl BufRead) -> Result<Vec<EditableDatapoint>> {
    let mut lines = reader.lines();

    // Skip header
    lines.next();

    let mut datapoints = Vec::new();
    let offset = UtcOffset::current_local_offset()?;

    for line in lines {
        let line = line?;
        let mut fields = line.split('\t');

        let date_str = fields
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing date"))?;
        let value_str = fields
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing value"))?;
        let comment = fields.next().unwrap_or("").to_string();
        let id = fields.next().map(String::from).filter(|s| !s.is_empty());

        let date = PrimitiveDateTime::parse(date_str, TIMESTAMP_FORMAT)?;
        let timestamp = date.assume_offset(offset).to_offset(UtcOffset::UTC);
        let value = value_str.parse()?;

        datapoints.push(EditableDatapoint {
            id,
            timestamp: Some(timestamp),
            value: Some(value),
            comment: Some(comment),
        });
    }

    Ok(datapoints)
}

pub async fn edit_datapoints(client: &BeeminderClient, goal: &str) -> Result<()> {
    let datapoints = client
        .get_datapoints(goal, Some("timestamp"), Some(20))
        .await?;

    // Create temp file with datapoints and let user edit it
    let mut temp = NamedTempFile::new()?;
    write_datapoints_tsv(&mut temp, &datapoints)?;
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "nvim".to_string());
    ProcessCommand::new(editor)
        .arg(temp.path())
        .status()
        .context("Failed to open editor")?;

    let reader = std::io::BufReader::new(File::open(temp.path())?);
    let edited_datapoints = read_datapoints_tsv(reader)?;
    let orig_map: HashMap<String, &Datapoint> =
        datapoints.iter().map(|dp| (dp.id.clone(), dp)).collect();
    let mut ids_to_delete: HashSet<String> = datapoints.iter().map(|dp| dp.id.clone()).collect();

    for dp in edited_datapoints {
        if let EditableDatapoint { id: Some(id), .. } = dp {
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
        } else {
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

    for id in ids_to_delete {
        println!("Deleting datapoint '{id}'.");
        client.delete_datapoint(goal, &id).await?;
    }

    Ok(())
}
