use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use anyhow::{
  Context,
  anyhow
};
use chrono::{
  DateTime,
  Datelike,
  Duration,
  LocalResult,
  NaiveDate,
  NaiveDateTime,
  TimeZone,
  Utc,
  Weekday
};
use chrono_tz::Tz;
use regex::Regex;
use serde::Deserialize;

const TIMEZONE_CONFIG_FILE: &str =
  "rivet.toml";
const TIMEZONE_ENV_VAR: &str =
  "RIVET_TIMEZONE";
const TIMEZONE_CONFIG_ENV_VAR: &str =
  "RIVET_CONFIG";
const LEGACY_TIMEZONE_CONFIG_ENV_VAR:
  &str = "RIVET_TIME_CONFIG";
const DEFAULT_PROJECT_TIMEZONE: &str =
  "America/Mexico_City";

#[derive(Debug, Deserialize)]
struct TimezoneConfig {
  timezone: Option<String>,
  time:     Option<TimezoneSection>
}

#[derive(Debug, Deserialize)]
struct TimezoneSection {
  timezone: Option<String>
}

pub fn project_timezone() -> &'static Tz
{
  static PROJECT_TZ: OnceLock<Tz> =
    OnceLock::new();
  PROJECT_TZ.get_or_init(
    resolve_project_timezone
  )
}

#[must_use]
pub fn to_project_date(
  dt: DateTime<Utc>
) -> chrono::NaiveDate {
  dt.with_timezone(project_timezone())
    .date_naive()
}

#[must_use]
pub fn format_project_date(
  dt: DateTime<Utc>
) -> String {
  dt.with_timezone(project_timezone())
    .format("%Y-%m-%d")
    .to_string()
}

fn resolve_project_timezone() -> Tz {
  if let Ok(raw) =
    std::env::var(TIMEZONE_ENV_VAR)
  {
    if let Some(tz) = parse_timezone(
      &raw,
      TIMEZONE_ENV_VAR
    ) {
      return tz;
    }
  }

  if let Some(path) =
    timezone_config_path()
    && let Some(tz) =
      load_timezone_from_file(&path)
  {
    return tz;
  }

  parse_timezone(
    DEFAULT_PROJECT_TIMEZONE,
    "DEFAULT_PROJECT_TIMEZONE"
  )
  .unwrap_or_else(|| {
    tracing::error!(
      "failed to parse fallback \
       timezone; using UTC"
    );
    chrono_tz::UTC
  })
}

fn timezone_config_path()
-> Option<PathBuf> {
  if let Ok(raw) = std::env::var(
    TIMEZONE_CONFIG_ENV_VAR
  ) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return Some(PathBuf::from(
        trimmed
      ));
    }
  }
  if let Ok(raw) = std::env::var(
    LEGACY_TIMEZONE_CONFIG_ENV_VAR
  ) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
      return Some(PathBuf::from(
        trimmed
      ));
    }
  }

  std::env::current_dir().ok().map(
    |dir| {
      dir.join(TIMEZONE_CONFIG_FILE)
    }
  )
}

fn load_timezone_from_file(
  path: &PathBuf
) -> Option<Tz> {
  if !path.exists() {
    tracing::info!(
      file = %path.display(),
      "timezone config file not found"
    );
    return None;
  }

  let raw = match fs::read_to_string(
    path
  ) {
    | Ok(raw) => raw,
    | Err(err) => {
      tracing::error!(
        file = %path.display(),
        error = %err,
        "failed reading timezone config file"
      );
      return None;
    }
  };

  let parsed = match toml::from_str::<
    TimezoneConfig
  >(&raw)
  {
    | Ok(parsed) => parsed,
    | Err(err) => {
      tracing::error!(
        file = %path.display(),
        error = %err,
        "failed parsing timezone config file"
      );
      return None;
    }
  };

  let timezone =
    parsed.timezone.or_else(|| {
      parsed.time.and_then(|section| {
        section.timezone
      })
    });
  let Some(timezone) = timezone else {
    tracing::warn!(
      file = %path.display(),
      "timezone config had no timezone field"
    );
    return None;
  };

  parse_timezone(
    timezone.as_str(),
    &format!("file:{}", path.display())
  )
}

fn parse_timezone(
  raw: &str,
  source: &str
) -> Option<Tz> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    tracing::warn!(
      source,
      "timezone source was empty"
    );
    return None;
  }

  match trimmed.parse::<Tz>() {
    | Ok(tz) => {
      tracing::info!(
        source,
        timezone = %trimmed,
        "configured project timezone"
      );
      Some(tz)
    }
    | Err(err) => {
      tracing::error!(
        source,
        timezone = %trimmed,
        error = %err,
        "failed to parse timezone id"
      );
      None
    }
  }
}

fn to_utc_from_project_local(
  local_naive: NaiveDateTime,
  context: &str
) -> anyhow::Result<DateTime<Utc>> {
  match project_timezone()
    .from_local_datetime(&local_naive)
  {
    | LocalResult::Single(local_dt) => {
      Ok(local_dt.with_timezone(&Utc))
    }
    | LocalResult::Ambiguous(
      first,
      second
    ) => {
      tracing::warn!(
        context,
        first = %first,
        second = %second,
        "ambiguous local datetime; using earliest"
      );
      let chosen = if first <= second {
        first
      } else {
        second
      };
      Ok(chosen.with_timezone(&Utc))
    }
    | LocalResult::None => {
      Err(anyhow!(
        "local datetime does not \
         exist in configured \
         timezone: {context}"
      ))
    }
  }
}

#[tracing::instrument(skip(now), fields(input = input))]
pub fn parse_date_expr(
  input: &str,
  now: DateTime<Utc>
) -> anyhow::Result<DateTime<Utc>> {
  let token = input.trim();
  let lower =
    token.to_ascii_lowercase();

  match lower.as_str() {
    | "now" => return Ok(now),
    | "today" => {
      let local_now = now
        .with_timezone(
          project_timezone()
        );
      let date = local_now.date_naive();
      let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| {
          anyhow!(
            "failed to construct \
             midnight for today"
          )
        })?;
      return to_utc_from_project_local(
        midnight, "today"
      );
    }
    | "tomorrow" => {
      let today =
        parse_date_expr("today", now)?;
      return Ok(
        today + Duration::days(1)
      );
    }
    | "yesterday" => {
      let today =
        parse_date_expr("today", now)?;
      return Ok(
        today - Duration::days(1)
      );
    }
    | _ => {}
  }

  if token.len() == 4
    && token
      .chars()
      .all(|c| c.is_ascii_digit())
  {
    let year: i32 =
      token.parse().context(
        "invalid 4-digit year"
      )?;
    let date = NaiveDate::from_ymd_opt(
      year, 1, 1
    )
    .ok_or_else(|| {
      anyhow!(
        "invalid year value: {year}"
      )
    })?;
    let midnight = date
      .and_hms_opt(0, 0, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct \
           midnight for year"
        )
      })?;
    return to_utc_from_project_local(
      midnight,
      "year-4digit"
    );
  }

  if let Some(target_weekday) =
    parse_weekday_name(&lower)
  {
    let local_now =
      now.with_timezone(
        project_timezone()
      );
    let local_today =
      local_now.date_naive();
    let target_date = next_weekday_date(
      local_today,
      target_weekday
    );
    let midnight = target_date
      .and_hms_opt(0, 0, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct \
           weekday midnight"
        )
      })?;
    return to_utc_from_project_local(
      midnight,
      "weekday-name"
    );
  }

  if let Some((hour, minute)) =
    parse_clock_time(token)
  {
    let local_now =
      now.with_timezone(
        project_timezone()
      );
    let mut day =
      local_now.date_naive();
    let local_candidate = day
      .and_hms_opt(hour, minute, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct clock \
           time candidate"
        )
      })?;
    if local_candidate
      <= local_now.naive_local()
    {
      day = day
        .checked_add_signed(
          Duration::days(1)
        )
        .ok_or_else(|| {
          anyhow!(
            "failed to advance to \
             next day"
          )
        })?;
    }
    let next_candidate = day
      .and_hms_opt(hour, minute, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct next \
           clock time candidate"
        )
      })?;
    return to_utc_from_project_local(
      next_candidate,
      "clock-time"
    );
  }

  if let Some(target_month) =
    parse_month_name(&lower)
  {
    let local_now =
      now.with_timezone(
        project_timezone()
      );
    let mut year = local_now.year();
    let candidate_this_year =
      NaiveDate::from_ymd_opt(
        year,
        target_month,
        1
      )
      .ok_or_else(|| {
        anyhow!(
          "invalid month value: \
           {target_month}"
        )
      })?
      .and_hms_opt(0, 0, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct month \
           candidate"
        )
      })?;

    if candidate_this_year
      <= local_now.naive_local()
    {
      year = year.saturating_add(1);
    }

    let candidate_next =
      NaiveDate::from_ymd_opt(
        year,
        target_month,
        1
      )
      .ok_or_else(|| {
        anyhow!(
          "invalid month/year \
           candidate"
        )
      })?
      .and_hms_opt(0, 0, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct month \
           midnight"
        )
      })?;

    return to_utc_from_project_local(
      candidate_next,
      "month-name"
    );
  }

  let rel_re = Regex::new(r"^(?P<sign>[+-])(?P<num>\d+)(?P<unit>[dhm])$")
        .map_err(|e| anyhow!("internal regex compile failure: {e}"))?;

  if let Some(caps) =
    rel_re.captures(token)
  {
    let sign = caps
      .name("sign")
      .map(|m| m.as_str())
      .ok_or_else(|| {
        anyhow!("missing relative sign")
      })?;
    let num: i64 = caps
      .name("num")
      .map(|m| m.as_str())
      .ok_or_else(|| {
        anyhow!(
          "missing relative amount"
        )
      })?
      .parse()
      .context(
        "invalid relative number"
      )?;
    let unit = caps
      .name("unit")
      .map(|m| m.as_str())
      .ok_or_else(|| {
        anyhow!("missing relative unit")
      })?;

    let duration = match unit {
      | "d" => Duration::days(num),
      | "h" => Duration::hours(num),
      | "m" => Duration::minutes(num),
      | _ => {
        return Err(anyhow!(
          "unknown relative unit: \
           {unit}"
        ))
      }
    };

    return Ok(
      if sign == "-" {
        now - duration
      } else {
        now + duration
      }
    );
  }

  if let Ok(ndt) =
    NaiveDateTime::parse_from_str(
      token,
      "%Y%m%dT%H%M%SZ"
    )
  {
    return Ok(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
  }

  if let Ok(dt) =
    DateTime::parse_from_rfc3339(token)
  {
    return Ok(dt.with_timezone(&Utc));
  }

  if let Ok(date) =
    NaiveDate::parse_from_str(
      token, "%Y-%m-%d"
    )
  {
    let midnight = date
      .and_hms_opt(0, 0, 0)
      .ok_or_else(|| {
        anyhow!(
          "failed to construct \
           midnight for date"
        )
      })?;
    return to_utc_from_project_local(
      midnight, "date"
    );
  }

  for fmt in
    ["%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M"]
  {
    if let Ok(ndt) =
      NaiveDateTime::parse_from_str(
        token, fmt
      )
    {
      return to_utc_from_project_local(
        ndt, fmt
      );
    }
  }

  Err(anyhow!(
    "unrecognized date expression: \
     {input}"
  ))
  .with_context(|| {
    "supported formats: \
     now/today/tomorrow/yesterday, \
     4-digit year, weekday names (e.g. \
     monday), month names (e.g. \
     march), clock times (e.g. 3:23pm \
     or 15:23), +Nd/+Nh/+Nm, RFC3339, \
     YYYY-MM-DD, YYYY-MM-DDTHH:MM, \
     YYYY-MM-DD HH:MM, YYYYMMDDTHHMMSSZ"
  })
}

fn parse_weekday_name(
  token: &str
) -> Option<Weekday> {
  match token.trim() {
    | "monday" | "mon" => {
      Some(Weekday::Mon)
    }
    | "tuesday" | "tue" | "tues" => {
      Some(Weekday::Tue)
    }
    | "wednesday" | "wed" => {
      Some(Weekday::Wed)
    }
    | "thursday" | "thu" | "thur"
    | "thurs" => Some(Weekday::Thu),
    | "friday" | "fri" => {
      Some(Weekday::Fri)
    }
    | "saturday" | "sat" => {
      Some(Weekday::Sat)
    }
    | "sunday" | "sun" => {
      Some(Weekday::Sun)
    }
    | _ => None
  }
}

fn next_weekday_date(
  from: NaiveDate,
  target: Weekday
) -> NaiveDate {
  let from_idx = from
    .weekday()
    .num_days_from_monday()
    as i64;
  let target_idx = target
    .num_days_from_monday()
    as i64;
  let mut delta =
    (7 + target_idx - from_idx) % 7;
  if delta == 0 {
    delta = 7;
  }
  from
    .checked_add_signed(Duration::days(
      delta
    ))
    .unwrap_or(from)
}

fn parse_clock_time(
  token: &str
) -> Option<(u32, u32)> {
  let clock_re = Regex::new(
    r"(?i)^(?P<hour>\d{1,2}):(?P<minute>\d{2})\s*(?P<ampm>[ap]m)?$",
  )
  .ok()?;
  let captures =
    clock_re.captures(token.trim())?;

  let raw_hour = captures
    .name("hour")?
    .as_str()
    .parse::<u32>()
    .ok()?;
  let minute = captures
    .name("minute")?
    .as_str()
    .parse::<u32>()
    .ok()?;
  if minute > 59 {
    return None;
  }

  let hour = if let Some(ampm_match) =
    captures.name("ampm")
  {
    let ampm = ampm_match
      .as_str()
      .to_ascii_lowercase();
    if raw_hour == 0 || raw_hour > 12 {
      return None;
    }
    match ampm.as_str() {
      | "am" => {
        if raw_hour == 12 {
          0
        } else {
          raw_hour
        }
      }
      | "pm" => {
        if raw_hour == 12 {
          12
        } else {
          raw_hour + 12
        }
      }
      | _ => return None
    }
  } else {
    if raw_hour > 23 {
      return None;
    }
    raw_hour
  };

  Some((hour, minute))
}

fn parse_month_name(
  token: &str
) -> Option<u32> {
  match token.trim() {
    | "january" | "jan" => Some(1),
    | "february" | "feb" => Some(2),
    | "march" | "mar" => Some(3),
    | "april" | "apr" => Some(4),
    | "may" => Some(5),
    | "june" | "jun" => Some(6),
    | "july" | "jul" => Some(7),
    | "august" | "aug" => Some(8),
    | "september" | "sep" | "sept" => {
      Some(9)
    }
    | "october" | "oct" => Some(10),
    | "november" | "nov" => Some(11),
    | "december" | "dec" => Some(12),
    | _ => None
  }
}

#[cfg(test)]
mod tests {
  use chrono::{
    TimeZone,
    Utc
  };

  use super::{
    parse_date_expr,
    to_project_date
  };

  #[test]
  fn parses_four_digit_year() {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 17, 12, 0, 0
      )
      .single()
      .expect("valid now");
    let parsed =
      parse_date_expr("2028", now)
        .expect("parse year");
    assert_eq!(
      to_project_date(parsed)
        .format("%Y-%m-%d")
        .to_string(),
      "2028-01-01"
    );
  }

  #[test]
  fn parses_weekday_name() {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 17, 12, 0, 0
      )
      .single()
      .expect("valid now");
    let parsed =
      parse_date_expr("wednesday", now)
        .expect("parse weekday");
    assert_eq!(
      to_project_date(parsed)
        .format("%Y-%m-%d")
        .to_string(),
      "2026-02-18"
    );
  }

  #[test]
  fn parses_month_name() {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 17, 12, 0, 0
      )
      .single()
      .expect("valid now");
    let parsed =
      parse_date_expr("march", now)
        .expect("parse month");
    assert_eq!(
      to_project_date(parsed)
        .format("%Y-%m-%d")
        .to_string(),
      "2026-03-01"
    );
  }

  #[test]
  fn parses_clock_time() {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 17, 23, 0, 0
      )
      .single()
      .expect("valid now");
    let parsed =
      parse_date_expr("3:23pm", now)
        .expect("parse clock time");
    assert_eq!(
      parsed
        .with_timezone(
          super::project_timezone()
        )
        .format("%H:%M")
        .to_string(),
      "15:23"
    );
  }
}

pub mod taskwarrior_date_serde {
  use chrono::{
    DateTime,
    NaiveDateTime,
    Utc
  };
  use serde::{
    Deserialize,
    Deserializer,
    Serializer
  };

  pub fn serialize<S>(
    dt: &DateTime<Utc>,
    serializer: S
  ) -> Result<S::Ok, S::Error>
  where
    S: Serializer
  {
    serializer.serialize_str(
      &dt
        .format("%Y%m%dT%H%M%SZ")
        .to_string()
    )
  }

  pub fn deserialize<'de, D>(
    deserializer: D
  ) -> Result<DateTime<Utc>, D::Error>
  where
    D: Deserializer<'de>
  {
    let raw = String::deserialize(
      deserializer
    )?;
    NaiveDateTime::parse_from_str(&raw, "%Y%m%dT%H%M%SZ")
            .map(|ndt| DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc))
            .map_err(serde::de::Error::custom)
  }

  pub mod option {
    use chrono::{
      DateTime,
      NaiveDateTime,
      Utc
    };
    use serde::{
      Deserialize,
      Deserializer,
      Serializer
    };

    pub fn serialize<S>(
      dt: &Option<DateTime<Utc>>,
      serializer: S
    ) -> Result<S::Ok, S::Error>
    where
      S: Serializer
    {
      match dt {
        | Some(value) => {
          super::serialize(
            value, serializer
          )
        }
        | None => {
          serializer.serialize_none()
        }
      }
    }

    pub fn deserialize<'de, D>(
      deserializer: D
    ) -> Result<
      Option<DateTime<Utc>>,
      D::Error
    >
    where
      D: Deserializer<'de>
    {
      let opt =
        Option::<String>::deserialize(
          deserializer
        )?;
      match opt {
                Some(raw) => NaiveDateTime::parse_from_str(&raw, "%Y%m%dT%H%M%SZ")
                    .map(|ndt| Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)))
                    .map_err(serde::de::Error::custom),
                None => Ok(None),
            }
    }
  }
}
