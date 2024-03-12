use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use clap::Parser;
use itertools::Itertools;
use serde::Deserialize;
use ureq::OrAnyStatus;

use std::{env, time::Duration};

fn main() {
    let args: Args = Args::parse();

    let abstract_api_key = require_from_env("ABSTRACT_API_KEY");
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
            let from_api = fetch_holidays_from_abstract(&abstract_api_key, cc, date);
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

const ABSTRACT_HOLIDAYS_API_URL: &str = "https://holidays.abstractapi.com/v1/";

fn fetch_holidays_from_abstract(
    api_key: &str,
    country: &str,
    date: NaiveDate,
) -> Result<Vec<Holiday>> {
    let result = ureq::get(ABSTRACT_HOLIDAYS_API_URL)
        .query("api_key", api_key)
        .query("country", country)
        .query("year", date.year_ce().1.to_string().as_str())
        .query("month", (date.month0() + 1).to_string().as_str())
        .query("day", (date.day0() + 1).to_string().as_str())
        .call()?
        .into_json::<Vec<Holiday>>()?
        .into_iter()
        .map(|mut h| {
            h.drop_empty_string_values();
            h
        })
        .collect_vec();

    // From what I've observed, the rate limit for the free tier of the API is 1 req/s.
    // The helpful AI built in to their documentation couldn't confirm that, though.
    // Sleeping for a second here is simple and should do the trick.
    std::thread::sleep(Duration::from_secs(1));

    Ok(result)
}

#[derive(Deserialize, Debug)]
struct Holiday {
    name: String,
    name_local: Option<String>,
    country: Option<String>,
    location: Option<String>,
}

impl Holiday {
    fn drop_empty_string_values(&mut self) {
        self.name_local = self.name_local.as_ref().filter(|v| !v.is_empty()).cloned();
        self.country = self.country.as_ref().filter(|v| !v.is_empty()).cloned();
        self.location = self.location.as_ref().filter(|v| !v.is_empty()).cloned();
    }
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

    let binding = holidays.iter().into_group_map_by(|h| h.location.clone());
    let mut holidays_by_location: Vec<(&Option<String>, &Vec<&Holiday>)> = binding.iter().collect();

    holidays_by_location.sort_by_key(|(location, _)| *location);

    for (location, holidays) in holidays_by_location {
        if let Some(location) = location {
            message_blocks.push(ureq::json!(
                {
                    "type": "section",
                    "text": {
                        "type": "mrkdwn",
                        "text": format!("_{}_", location),
                    }
                }
            ));

            let holiday_lines: Vec<serde_json::Value> = holidays
                .iter()
                .map(|h| {
                    let mut elements: Vec<serde_json::Value> = Vec::new();

                    elements.push(ureq::json!({
                        "type": "text",
                        "text": h.name,
                        "style": {
                            "bold": true
                        }
                    }));

                    if let Some(name_local) = &h.name_local {
                        elements.push(ureq::json!({
                            "type": "text",
                            "text": format!(" ({})", name_local),
                            "style": {
                                "italic": true
                            }
                        }));
                    }

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
    }

    let message = ureq::json!({
        "blocks": message_blocks,
    });

    println!("{}", serde_json::to_string_pretty(&message).unwrap());

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
