//! What the window knows about the daemon and when it asks again.

use std::time::{Duration, Instant};

use ullage_cli::usage_view::{FetchError, UsageFetch};
use ullage_protocol::{CONTROL_PROTOCOL_VERSION, SnapshotPayload};

/// Seconds between the end of one fetch and the start of the next.
pub const REFRESH_SECS: u64 = 120;
const REFRESH: Duration = Duration::from_secs(REFRESH_SECS);

/// Words for a failed fetch that left the window with nothing to show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub title: String,
    pub body: String,
    /// The command the user can run, shown in the command box.
    pub command: &'static str,
}

pub fn failure_for(error: &FetchError) -> Failure {
    let (title, body, command): (&str, String, &str) = match error {
        FetchError::DaemonUnavailable => (
            "Can't reach the ullage daemon",
            "Install it once from a terminal. This window tries again every 2 minutes and \
             shows your usage as soon as it answers."
                .into(),
            "ullage daemon install",
        ),
        FetchError::InvalidEndpoint => (
            "The ullage control endpoint is invalid",
            "ULLAGE_CONTROL_PIPE points somewhere this app can't use. Unset it to use the \
             default, then check the daemon."
                .into(),
            "ullage daemon status",
        ),
        FetchError::InvalidResponse => (
            "The ullage daemon gave an answer this app can't read",
            "Update ullage and this app to the same version, then restart the daemon.".into(),
            "ullage daemon install",
        ),
        FetchError::ProtocolMismatch {
            supported_version, ..
        } => (
            "This app and the ullage daemon don't match",
            format!(
                "The daemon speaks protocol version {supported_version} and this app speaks \
                 {CONTROL_PROTOCOL_VERSION}. Update ullage and this app to the same version."
            ),
            "ullage daemon install",
        ),
        FetchError::Daemon(_) => (
            "The ullage daemon reported an error",
            "Run the command below in a terminal to see the details.".into(),
            "ullage show",
        ),
    };
    Failure {
        title: title.into(),
        body,
        command,
    }
}

/// What the window has to show.
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// No result yet.
    Loading,
    /// The last fetch worked.
    Ready(Vec<SnapshotPayload>),
    /// The last fetch failed; the data of an earlier one is kept.
    Stalled(Vec<SnapshotPayload>),
    /// No fetch ever worked, and the last one failed.
    Unavailable(Failure),
}

impl State {
    pub fn after_fetch(self, result: Result<UsageFetch, FetchError>) -> State {
        match result {
            Ok(fetch) => State::Ready(fetch.snapshots),
            Err(error) => match self {
                State::Ready(snapshots) | State::Stalled(snapshots) => State::Stalled(snapshots),
                State::Loading | State::Unavailable(_) => State::Unavailable(failure_for(&error)),
            },
        }
    }

    /// The snapshots to draw cards from, when there are any to draw.
    pub fn snapshots(&self) -> Option<&[SnapshotPayload]> {
        match self {
            State::Ready(snapshots) | State::Stalled(snapshots) => Some(snapshots),
            State::Loading | State::Unavailable(_) => None,
        }
    }

    /// Whether the title bar counts down to the next fetch. After a failed
    /// fetch it shows an empty ring and `0m00s` until the retry starts.
    fn counts_down(&self) -> bool {
        matches!(self, State::Ready(_))
    }
}

/// When the next fetch starts. At most one fetch runs at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Schedule {
    fetching: bool,
    next_at: Instant,
}

impl Schedule {
    /// A schedule whose first fetch is due at once.
    pub fn new(now: Instant) -> Self {
        Self {
            fetching: false,
            next_at: now,
        }
    }

    /// Whether a fetch is due and none is running.
    pub fn due(&self, now: Instant) -> bool {
        !self.fetching && now >= self.next_at
    }

    pub fn is_fetching(&self) -> bool {
        self.fetching
    }

    pub fn start(&mut self) {
        self.fetching = true;
    }

    /// A fetch ended, with a result or without. The next one is a full
    /// interval away.
    pub fn finish(&mut self, now: Instant) {
        self.fetching = false;
        self.next_at = now + REFRESH;
    }

    /// Makes a fetch due now. A fetch that is already running is enough, and
    /// the interval starts when it ends.
    pub fn request_now(&mut self, now: Instant) {
        if !self.fetching {
            self.next_at = now;
        }
    }

    fn remaining(&self, now: Instant) -> Duration {
        if self.fetching {
            Duration::ZERO
        } else {
            self.next_at.saturating_duration_since(now)
        }
    }
}

/// The title bar's countdown: the filled part of the ring, from 0 to 1, and
/// the whole seconds on the label.
pub fn countdown(state: &State, schedule: &Schedule, now: Instant) -> (f32, u64) {
    if !state.counts_down() {
        return (0.0, 0);
    }
    let remaining = schedule.remaining(now);
    let fraction = (remaining.as_secs_f32() / REFRESH_SECS as f32).clamp(0.0, 1.0);
    (
        fraction,
        remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0),
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use ullage_protocol::{ControlResult, ProviderId, QueryOutcome, SubscriptionUsage};

    use super::*;

    fn fetch(count: usize) -> UsageFetch {
        let now = Utc.with_ymd_and_hms(2026, 9, 20, 12, 0, 0).unwrap();
        let snapshot = SnapshotPayload {
            account_id: "a".into(),
            usage: QueryOutcome::Complete {
                data: SubscriptionUsage {
                    provider: ProviderId::new("claude"),
                    account_label: None,
                    plan: None,
                    subscription_expires_at: None,
                    observed_at: now,
                    windows: Vec::new(),
                },
            },
            last_success_at: now,
            stale: false,
            last_error: None,
            last_error_at: None,
            metrics: Vec::new(),
        };
        UsageFetch {
            snapshots: vec![snapshot; count],
            daemon_version: None,
        }
    }

    #[test]
    fn five_failure_kinds_have_their_own_words() {
        let kinds = [
            FetchError::DaemonUnavailable,
            FetchError::InvalidEndpoint,
            FetchError::InvalidResponse,
            FetchError::ProtocolMismatch {
                supported_version: 9,
                reply_version: 9,
                result: Box::new(ControlResult::ProtocolMismatch {
                    supported_version: 9,
                }),
            },
            FetchError::Daemon(Box::new(ControlResult::Snapshots(Vec::new()))),
        ];
        let copies: Vec<Failure> = kinds.iter().map(failure_for).collect();
        let titles: Vec<&str> = copies.iter().map(|copy| copy.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Can't reach the ullage daemon",
                "The ullage control endpoint is invalid",
                "The ullage daemon gave an answer this app can't read",
                "This app and the ullage daemon don't match",
                "The ullage daemon reported an error",
            ]
        );
        let commands: Vec<&str> = copies.iter().map(|copy| copy.command).collect();
        assert_eq!(
            commands,
            [
                "ullage daemon install",
                "ullage daemon status",
                "ullage daemon install",
                "ullage daemon install",
                "ullage show"
            ]
        );
        assert!(
            copies[0]
                .body
                .starts_with("Install it once from a terminal.")
        );
        assert!(
            copies[1]
                .body
                .starts_with("ULLAGE_CONTROL_PIPE points somewhere")
        );
        assert_eq!(
            copies[3].body,
            format!(
                "The daemon speaks protocol version 9 and this app speaks {CONTROL_PROTOCOL_VERSION}. \
                 Update ullage and this app to the same version."
            )
        );
        assert_eq!(
            copies[4].body,
            "Run the command below in a terminal to see the details."
        );
    }

    #[test]
    fn a_fetch_that_works_gives_ready_even_with_no_snapshots() {
        assert_eq!(
            State::Loading.after_fetch(Ok(fetch(0))),
            State::Ready(Vec::new())
        );
        assert_eq!(
            State::Unavailable(failure_for(&FetchError::DaemonUnavailable))
                .after_fetch(Ok(fetch(2)))
                .snapshots()
                .map(<[_]>::len),
            Some(2)
        );
    }

    #[test]
    fn a_failure_without_data_is_unavailable() {
        let state = State::Loading.after_fetch(Err(FetchError::InvalidEndpoint));
        assert_eq!(
            state,
            State::Unavailable(failure_for(&FetchError::InvalidEndpoint))
        );
        // The next failure replaces the words.
        let state = state.after_fetch(Err(FetchError::DaemonUnavailable));
        assert_eq!(
            state,
            State::Unavailable(failure_for(&FetchError::DaemonUnavailable))
        );
    }

    #[test]
    fn a_failure_with_data_keeps_the_data() {
        let ready = State::Ready(fetch(2).snapshots);
        let stalled = ready.after_fetch(Err(FetchError::DaemonUnavailable));
        assert!(matches!(&stalled, State::Stalled(snapshots) if snapshots.len() == 2));
        let again = stalled.after_fetch(Err(FetchError::InvalidResponse));
        assert!(matches!(&again, State::Stalled(snapshots) if snapshots.len() == 2));
        assert!(matches!(again.after_fetch(Ok(fetch(1))), State::Ready(s) if s.len() == 1));
    }

    #[test]
    fn the_first_fetch_is_due_at_once_and_runs_alone() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        assert!(schedule.due(now));
        schedule.start();
        assert!(schedule.is_fetching());
        assert!(!schedule.due(now + Duration::from_secs(500)));
    }

    #[test]
    fn the_next_fetch_is_an_interval_after_the_last_one_ended() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.start();
        let ended = now + Duration::from_secs(30);
        schedule.finish(ended);
        assert!(!schedule.due(ended + Duration::from_secs(119)));
        assert!(schedule.due(ended + Duration::from_secs(120)));
    }

    #[test]
    fn refresh_now_makes_a_fetch_due_and_restarts_the_interval_after_it() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.start();
        schedule.finish(now);
        let later = now + Duration::from_secs(40);
        assert!(!schedule.due(later));
        schedule.request_now(later);
        assert!(schedule.due(later));
        schedule.start();
        schedule.finish(later + Duration::from_secs(2));
        assert!(!schedule.due(later + Duration::from_secs(100)));
        assert!(schedule.due(later + Duration::from_secs(122)));
    }

    #[test]
    fn refresh_now_during_a_fetch_changes_nothing() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.start();
        schedule.request_now(now);
        assert!(!schedule.due(now));
        assert!(schedule.is_fetching());
    }

    #[test]
    fn the_countdown_runs_from_two_minutes_to_zero() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.start();
        schedule.finish(now);
        let ready = State::Ready(Vec::new());
        assert_eq!(countdown(&ready, &schedule, now), (1.0, 120));
        let (fraction, seconds) = countdown(&ready, &schedule, now + Duration::from_millis(28_500));
        assert_eq!(seconds, 92);
        assert!((fraction - 91.5 / 120.0).abs() < 1e-4);
        assert_eq!(
            countdown(&ready, &schedule, now + Duration::from_secs(500)),
            (0.0, 0)
        );
    }

    #[test]
    fn the_countdown_is_empty_while_a_fetch_runs_and_after_a_failure() {
        let now = Instant::now();
        let mut schedule = Schedule::new(now);
        schedule.start();
        assert_eq!(
            countdown(&State::Ready(Vec::new()), &schedule, now),
            (0.0, 0)
        );
        schedule.finish(now);
        let stalled = State::Stalled(Vec::new());
        assert_eq!(countdown(&stalled, &schedule, now), (0.0, 0));
        let unavailable = State::Unavailable(failure_for(&FetchError::DaemonUnavailable));
        assert_eq!(countdown(&unavailable, &schedule, now), (0.0, 0));
        assert_eq!(countdown(&State::Loading, &schedule, now), (0.0, 0));
    }
}
