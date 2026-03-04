# Uptime monitoring

We use [UptimeRobot](https://uptimerobot.com/) (free tier) to monitor `getcmdr.com`.

## What's monitored

- **HTTP monitor** on `https://getcmdr.com` — checks every 5 minutes from multiple regions.

## Alerts

- **Email**: Goes to the account owner's email (default alert contact).
- **Pushover**: Push notifications to phone via [Pushover](https://pushover.net/). Configured as an alert contact in UptimeRobot.

## Links

- **Dashboard**: https://uptimerobot.com/dashboard (login required)
- **Public status page**: https://stats.uptimerobot.com/MHKbVOfrcB

## Notes

- Free tier allows 50 monitors at 5-minute intervals.
- If we need to monitor more endpoints later (for example, the license server), add them in the UptimeRobot dashboard.
