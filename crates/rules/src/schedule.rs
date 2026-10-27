//! Schedules defining active time windows and day-of-week constraints.

use crate::types::{DayOfWeek, TimeOfDay};
use serde::{Deserialize, Serialize};

/// Schedule defining when a rule is permitted to fire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
    /// Optional start time of active window (e.g. "12:00").
    pub start_time: Option<TimeOfDay>,
    /// Optional end time of active window (e.g. "14:00").
    pub end_time: Option<TimeOfDay>,
    /// Days of the week when rule is active (empty implies active every day).
    pub days_of_week: Vec<DayOfWeek>,
}

impl Schedule {
    /// Creates an active schedule with specified start and end time strings ("HH:MM").
    pub fn time_range(start: &str, end: &str) -> crate::Result<Self> {
        Ok(Self {
            start_time: Some(TimeOfDay::parse(start)?),
            end_time: Some(TimeOfDay::parse(end)?),
            days_of_week: Vec::new(),
        })
    }

    /// Adds day-of-week filters to the schedule.
    pub fn with_days(mut self, days: Vec<DayOfWeek>) -> Self {
        self.days_of_week = days;
        self
    }

    /// Evaluates whether the current context time falls within this schedule.
    pub fn is_active(&self, time: &TimeOfDay, current_dow: Option<DayOfWeek>) -> bool {
        if !self.days_of_week.is_empty() {
            let Some(dow) = current_dow else {
                return false;
            };
            if !self.days_of_week.contains(&dow) {
                return false;
            }
        }

        let sec = time.as_seconds();

        if let (Some(start), Some(end)) = (&self.start_time, &self.end_time) {
            let start_sec = start.as_seconds();
            let end_sec = end.as_seconds();

            if start_sec <= end_sec {
                sec >= start_sec && sec <= end_sec
            } else {
                // Overnight schedule (e.g. 22:00 to 06:00)
                sec >= start_sec || sec <= end_sec
            }
        } else if let Some(start) = &self.start_time {
            sec >= start.as_seconds()
        } else if let Some(end) = &self.end_time {
            sec <= end.as_seconds()
        } else {
            true
        }
    }
}
