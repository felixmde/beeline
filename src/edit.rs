use crate::EditableDatapoint;
use anyhow::Result;
use beeminder::types::Datapoint;
use std::io::{BufRead, Write};
use time::macros::format_description;
use time::{PrimitiveDateTime, UtcOffset};

const TIMESTAMP_FORMAT: &[time::format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

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
