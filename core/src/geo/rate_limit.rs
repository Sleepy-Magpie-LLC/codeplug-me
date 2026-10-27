//! A local cap on RepeaterBook requests, so the app cannot be scripted into
//! pulling RepeaterBook's whole database one state at a time.
//!
//! The times of recent requests are kept in a small file in the host's cache
//! directory, so the limits hold across app restarts. The check itself is
//! pure, and is tested without touching the disk.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// The history file inside the cache directory.
const FILE: &str = "repeaterbook-requests.json";
/// Shortest gap between two requests, in seconds.
pub const MIN_GAP_SECS: u64 = 10;
/// Requests allowed in any rolling hour.
pub const PER_HOUR: usize = 10;
/// Requests allowed in any rolling day.
pub const PER_DAY: usize = 30;

const HOUR: u64 = 60 * 60;
const DAY: u64 = 24 * HOUR;

/// Whether a request at `now` fits the limits, given earlier request times
/// (unix seconds). The error says how long to wait.
///
/// Times in the future still count, so setting the clock back does not reset
/// the limits; ones more than a day off either way are dropped as stale.
pub fn check(history: &[u64], now: u64) -> Result<(), String> {
    let recent: Vec<u64> = history
        .iter()
        .copied()
        .filter(|&t| t.abs_diff(now) < DAY)
        .collect();
    let wait_for = |window: u64, limit: usize| -> Option<u64> {
        let mut in_window: Vec<u64> = recent
            .iter()
            .copied()
            .filter(|&t| t + window > now)
            .collect();
        if in_window.len() < limit {
            return None;
        }
        // The window frees a slot once enough of its oldest requests age out.
        in_window.sort_unstable();
        Some(in_window[in_window.len() - limit] + window - now)
    };
    if let Some(wait) = wait_for(DAY, PER_DAY) {
        return Err(format!(
            "RepeaterBook searches are limited to {PER_DAY} a day. Try again in {}.",
            describe(wait)
        ));
    }
    if let Some(wait) = wait_for(HOUR, PER_HOUR) {
        return Err(format!(
            "RepeaterBook searches are limited to {PER_HOUR} an hour. Try again in {}.",
            describe(wait)
        ));
    }
    if let Some(wait) = wait_for(MIN_GAP_SECS, 1) {
        return Err(format!(
            "Please wait {} before searching RepeaterBook again.",
            describe(wait)
        ));
    }
    Ok(())
}

/// A wait as "45 seconds", "12 minutes" or "3 hours", rounded up.
fn describe(secs: u64) -> String {
    let (n, unit) = if secs < 60 {
        (secs.max(1), "second")
    } else if secs < HOUR {
        (secs.div_ceil(60), "minute")
    } else {
        (secs.div_ceil(HOUR), "hour")
    };
    format!("{n} {unit}{}", if n == 1 { "" } else { "s" })
}

/// Check the limits for a request now and, if it is allowed, record it.
///
/// Requests are recorded before they are sent, so failed ones count too and
/// retrying in a loop cannot get around the limits. A missing or unreadable
/// history starts empty; a history that cannot be saved refuses the request,
/// since the limits could not be enforced otherwise.
pub fn acquire(cache_dir: &Path) -> Result<(), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("system clock is before 1970: {e}"))?
        .as_secs();
    let path = cache_dir.join(FILE);
    let mut history: Vec<u64> = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    check(&history, now)?;
    history.retain(|&t| t.abs_diff(now) < DAY);
    history.push(now);
    let body = serde_json::to_vec(&history).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(cache_dir)
        .and_then(|_| std::fs::write(&path, body))
        .map_err(|e| format!("could not record the RepeaterBook request: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000;

    #[test]
    fn first_request_is_allowed() {
        assert!(check(&[], NOW).is_ok());
    }

    #[test]
    fn requests_too_close_together_are_refused() {
        let err = check(&[NOW - 3], NOW).unwrap_err();
        assert!(err.contains("7 seconds"), "{err}");
        assert!(check(&[NOW - MIN_GAP_SECS], NOW).is_ok());
    }

    #[test]
    fn hourly_cap_waits_for_the_oldest_to_age_out() {
        // Ten requests, one every five minutes, the oldest 50 minutes ago.
        let history: Vec<u64> = (0..10).map(|i| NOW - 2700 + i * 300 - 300).collect();
        let err = check(&history, NOW).unwrap_err();
        assert!(err.contains("10 an hour"), "{err}");
        assert!(err.contains("10 minutes"), "{err}");
        assert!(check(&history, history[0] + HOUR).is_ok());
    }

    #[test]
    fn daily_cap_applies_across_hours() {
        // Thirty requests spread over the last fifteen hours, two an hour.
        let history: Vec<u64> = (0..30).map(|i| NOW - 15 * HOUR + i * 1800).collect();
        let err = check(&history, NOW + 60).unwrap_err();
        assert!(err.contains("30 a day"), "{err}");
        assert!(err.contains("9 hours"), "{err}");
    }

    #[test]
    fn old_requests_stop_counting() {
        let history: Vec<u64> = (0..50).map(|i| NOW - DAY - 1 - i).collect();
        assert!(check(&history, NOW).is_ok());
    }

    #[test]
    fn setting_the_clock_back_does_not_reset_the_limits() {
        assert!(check(&[NOW + 600], NOW).is_err());
    }

    #[test]
    fn acquire_records_and_then_enforces_the_gap() {
        let dir = std::env::temp_dir().join(format!("rb-rate-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        acquire(&dir).unwrap();
        let err = acquire(&dir).unwrap_err();
        assert!(err.contains("wait"), "{err}");
        let saved: Vec<u64> =
            serde_json::from_slice(&std::fs::read(dir.join(FILE)).unwrap()).unwrap();
        assert_eq!(saved.len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_history_starts_fresh() {
        let dir = std::env::temp_dir().join(format!("rb-rate-corrupt-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE), b"not json").unwrap();
        acquire(&dir).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
