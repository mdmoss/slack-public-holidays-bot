# Public Holiday Slackbot

This is a tiny utility to post public holiday information to Slack each day.

Public holiday information is pulled from the [Abstract API Public Holidays API](https://www.abstractapi.com/api/holidays-api). There's a generous free quota that you probably won't exceed.

## Setup and configuration

- Download or build a binary (TODO: set up GitHub actions to publish automatically.)

- Provide three environment variables:
    - `ABSTRACT_API_KEY`: API key for your Abstract API account.
    - `SLACK_WEBHOOK_URL`: URL of a Slack "Incoming Webhook" integration.

- Run the bot periodically. Cron is likely the easiest way, but you're free to choose your own adventure here.

## Usage

```
Usage: public-holiday-slackbot [OPTIONS] <COUNTRIES>

Arguments:
  <COUNTRIES>  comma-separated list of countries to fetch in 2-letter format (ISO 3166-1 alpha-2, e.g. "US,UK,AU")

Options:
      --date <DATE>  date to fetch in ISO8601 format (defaults to current day)
  -h, --help         Print help
```
