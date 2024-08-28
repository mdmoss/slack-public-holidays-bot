use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use clap::Parser;
use itertools::Itertools;
use serde::Deserialize;
use ureq::OrAnyStatus;
use isocountry;

use std::{env, time::Duration};

// There's a list of allowed types at https://date.nager.at/Api.
const ALLOWED_HOLIDAY_TYPES: &[&str] = &["Public", "Optional"];

fn main() {
    let args: Args = Args::parse();

    let slack_webhook_url = require_from_env("SLACK_WEBHOOK_URL");

    let date = match args.date {
        Some(date) => chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
            .expect("Invalid date argument (expected YYYY-MM-DD)"),
        None => chrono::Local::now().date_naive(),
    };

    println!("sending holidays for {}", date);

    let country_codes = args.countries.split(',');

    let holidays: Vec<Holiday> = country_codes
        .flat_map(|cc| {
            let from_api = fetch_holidays_from_nager(cc, date);
            match from_api {
                Ok(results) => results,
                Err(e) => {
                    println!("warning: error fetching holidays for country {}", cc);
                    println!("{}", e);
                    Vec::new()
                }
            }
        })
        .collect();

    for h in holidays.iter() {
        println!("{:?}", h);
    }

    send_to_slack(&slack_webhook_url, holidays).unwrap();
}

#[derive(Parser)]
struct Args {
    #[arg(long)]
    #[arg(help("date to fetch in ISO8601 format (defaults to current day)"))]
    date: Option<String>,
    #[arg(help("comma-separated list of countries to fetch in 2-letter format (ISO 3166-1 alpha-2, e.g. \"US,UK,AU\")"))]
    countries: String,
}

const NAGER_HOLIDAYS_URL_BASE: &str = "https://date.nager.at/api/v3/PublicHolidays";

fn fetch_holidays_from_nager(
    country: &str,
    date: NaiveDate,
) -> Result<Vec<Holiday>> {
    let year = date.year_ce().1;
    let url = format!("{NAGER_HOLIDAYS_URL_BASE}/{year}/{country}");
    let result = ureq::get(&url)
        .call()?
        .into_json::<Vec<Holiday>>()?
        .into_iter()
        .filter(|h| {
            h.date == date.to_string() &&
            h.types.iter().any(|t| ALLOWED_HOLIDAY_TYPES.iter().any(|allowed_type| allowed_type == t))
        })
        .collect_vec();

    Ok(result)
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Holiday {
    date: String,
    name: String,
    local_name: String,
    types: Vec<String>,
    counties: Option<Vec<String>>,
    country_code: String,
}

fn require_from_env(key: &str) -> String {
    env::var(key).unwrap_or_else(|_| panic!("missing required environment variable: {}", key))
}

fn send_to_slack(webhook_url: &str, holidays: Vec<Holiday>) -> Result<()> {
    if holidays.is_empty() {
        return Ok(());
    }

    let mut message_blocks = Vec::new();
    message_blocks.push(ureq::json!(
        {
            "type": "header",
            "text": {
                "type": "plain_text",
                "text": ":calendar: Holidays",
                "emoji": true
            }
        }
    ));

    let binding = holidays.iter().into_group_map_by(|h| h.country_code.clone());
    let mut holidays_by_country: Vec<(&String, &Vec<&Holiday>)> = binding.iter().collect();

    holidays_by_country.sort_by_key(|(location, _)| *location);

    for (location, holidays) in holidays_by_country {

        let country_name = isocountry::CountryCode::for_alpha2_caseless(&location).map(|c| c.name()).unwrap_or(location);

        message_blocks.push(ureq::json!(
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("_{}_", country_name),
                }
            }
        ));

        let holiday_lines: Vec<serde_json::Value> = holidays
            .iter()
            .map(|h| {
                let mut elements: Vec<serde_json::Value> = Vec::new();

                elements.push(ureq::json!({
                    "type": "text",
                    "text": h.local_name,
                    "style": {
                        "bold": true
                    }
                }));

                elements.push(ureq::json!({
                    "type": "text",
                    "text": format!(" ({})", h.name),
                    "style": {
                        "italic": true
                    }
                }));

                ureq::json!({
                    "type": "rich_text_section",
                    "elements": elements,
                })
            })
            .collect();

        message_blocks.push(ureq::json!(
            {
                "type": "rich_text",
                "elements": [
                    {
                    "type": "rich_text_list",
                    "style": "bullet",
                    "elements": holiday_lines,
                }]
            }
        ))

    }

    let message = ureq::json!({
        "blocks": message_blocks,
    });

    // println!("{}", serde_json::to_string_pretty(&message).unwrap());

    let resp = ureq::post(webhook_url)
        .send_json(&message)
        .or_any_status()?;

    if resp.status() >= 400 {
        println!(
            "Warning: slack request failed (status {})",
            resp.status_text()
        );
        println!("request\n{}\n", serde_json::to_string_pretty(&message)?);
        println!("response\n{}\n", resp.into_string()?);
        return Err(anyhow::format_err!("request to Slack API failed"));
    }

    Ok(())
}
