use chrono::{DateTime, Utc};
use ullage_cli::usage_view::usage_cards;
use ullage_protocol::{
    MeasurementUnit, PartialFailure, ProviderId, QueryOutcome, SnapshotPayload, SubscriptionUsage,
    UsageMeasurement, UsageWindow, UsageWindowKind,
};

use super::*;

pub(crate) fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_800_000_000, 0).unwrap()
}

/// A text of exactly `bytes` bytes that is made of two-byte characters as far
/// as it can be, so that its length in characters is under its length in
/// bytes.
fn exactly(bytes: usize) -> String {
    let mut text = "é".repeat(bytes / 2);
    if bytes % 2 == 1 {
        text.push('a');
    }
    assert_eq!(text.len(), bytes);
    text
}

pub(crate) fn reading(name: &str) -> UsageMeasurement {
    UsageMeasurement {
        name: name.into(),
        used: 1.0,
        limit: Some(2.0),
        unit: MeasurementUnit::Percent,
    }
}

pub(crate) fn window(kind: UsageWindowKind, measurements: Vec<UsageMeasurement>) -> UsageWindow {
    UsageWindow {
        window: kind,
        resets_at: None,
        measurements,
    }
}

pub(crate) fn other(id: &str, label: &str) -> UsageWindowKind {
    UsageWindowKind::Other {
        id: id.into(),
        label: label.into(),
    }
}

pub(crate) fn snapshot(account_id: &str, windows: Vec<UsageWindow>) -> SnapshotPayload {
    SnapshotPayload {
        account_id: account_id.into(),
        usage: QueryOutcome::Complete {
            data: SubscriptionUsage {
                provider: ProviderId::new("claude"),
                account_label: Some("work".into()),
                plan: Some("max".into()),
                subscription_expires_at: None,
                observed_at: now(),
                windows,
            },
        },
        last_success_at: now(),
        stale: false,
        last_error: None,
        last_error_at: None,
        metrics: Vec::new(),
    }
}

/// The data of a snapshot made by [`snapshot`].
pub(crate) fn usage_mut(snapshot: &mut SnapshotPayload) -> &mut SubscriptionUsage {
    match &mut snapshot.usage {
        QueryOutcome::Complete { data } | QueryOutcome::Partial { data, .. } => data,
    }
}

fn first_reading(snapshot: &mut SnapshotPayload) -> &mut UsageMeasurement {
    &mut usage_mut(snapshot).windows[0].measurements[0]
}

/// One snapshot with one window and one reading, ready to be changed.
fn small() -> SnapshotPayload {
    snapshot("a1", vec![window(other("w", "W"), vec![reading("usage")])])
}

fn failure() -> PartialFailure {
    PartialFailure {
        scope: "scope".into(),
        message: "message".into(),
    }
}

/// Asserts that a reply is over a limit by one, and not over it at the limit.
fn limit_is_sharp(at: Vec<SnapshotPayload>, over: Vec<SnapshotPayload>, what: &str) {
    assert!(within_limits(&at), "{what}: at the limit");
    assert!(!within_limits(&over), "{what}: one over the limit");
}

#[test]
fn a_reply_of_the_size_a_daemon_gives_is_within_the_limits() {
    assert!(within_limits(&[]));
    assert!(within_limits(&[small()]));
}

#[test]
fn the_number_of_snapshots_is_limited() {
    let many = |count: usize| {
        (0..count)
            .map(|index| snapshot(&format!("a{index}"), Vec::new()))
            .collect::<Vec<_>>()
    };
    limit_is_sharp(many(MAX_SNAPSHOTS), many(MAX_SNAPSHOTS + 1), "snapshots");
}

#[test]
fn the_number_of_windows_of_a_snapshot_is_limited() {
    // A window of a kind with a fixed name, so that this is not the limit of
    // the readings or of the expanded size.
    let windows = |count: usize| {
        let each = window(UsageWindowKind::Weekly, vec![reading("usage")]);
        vec![snapshot("a1", vec![each; count])]
    };
    limit_is_sharp(windows(MAX_WINDOWS), windows(MAX_WINDOWS + 1), "windows");
}

#[test]
fn the_number_of_readings_of_a_window_is_limited() {
    let readings = |count: usize| {
        let names = (0..count).map(|index| reading(&format!("r{index}")));
        vec![snapshot(
            "a1",
            vec![window(other("w", "W"), names.collect())],
        )]
    };
    limit_is_sharp(
        readings(MAX_MEASUREMENTS_PER_WINDOW),
        readings(MAX_MEASUREMENTS_PER_WINDOW + 1),
        "readings of a window",
    );
}

#[test]
fn the_number_of_readings_of_the_reply_is_limited() {
    // Every window and every snapshot is within its own limit: only the sum
    // is over.
    let reply = |total: usize| {
        let per_snapshot = MAX_MEASUREMENTS_PER_WINDOW;
        let mut left = total;
        let mut snapshots = Vec::new();
        while left > 0 {
            let count = left.min(per_snapshot);
            let names = (0..count).map(|index| reading(&format!("r{index}")));
            let windows = vec![window(other("w", "W"), names.collect())];
            snapshots.push(snapshot(&format!("a{}", snapshots.len()), windows));
            left -= count;
        }
        snapshots
    };
    assert!(reply(MAX_MEASUREMENTS).len() <= MAX_SNAPSHOTS);
    limit_is_sharp(
        reply(MAX_MEASUREMENTS),
        reply(MAX_MEASUREMENTS + 1),
        "readings of the reply",
    );
}

/// For every text that the limit applies to, one that is `MAX_STRING_BYTES` long is
/// within the limit and one a byte longer is over it. The lengths are in
/// bytes: the long one is also under the limit in characters, so a check by
/// characters would let it pass.
#[test]
fn the_length_of_every_string_is_limited_in_bytes() {
    type Set = fn(&mut SnapshotPayload, String);
    let strings: [(&str, Set); 10] = [
        ("account_id", |snapshot, text| snapshot.account_id = text),
        ("provider", |snapshot, text| {
            usage_mut(snapshot).provider = ProviderId::new(text);
        }),
        ("account_label", |snapshot, text| {
            usage_mut(snapshot).account_label = Some(text);
        }),
        ("plan", |snapshot, text| {
            usage_mut(snapshot).plan = Some(text);
        }),
        ("window id", |snapshot, text| {
            usage_mut(snapshot).windows[0].window = other(&text, "W");
        }),
        ("window label", |snapshot, text| {
            usage_mut(snapshot).windows[0].window = other("w", &text);
        }),
        ("reading name", |snapshot, text| {
            first_reading(snapshot).name = text;
        }),
        ("currency code", |snapshot, text| {
            first_reading(snapshot).unit = MeasurementUnit::Currency { code: text };
        }),
        ("unit id", |snapshot, text| {
            first_reading(snapshot).unit = MeasurementUnit::Other {
                id: text,
                label: "L".into(),
            };
        }),
        ("unit label", |snapshot, text| {
            first_reading(snapshot).unit = MeasurementUnit::Other {
                id: "i".into(),
                label: text,
            };
        }),
    ];
    for (what, set) in strings {
        let with = |bytes: usize| {
            let mut one = small();
            set(&mut one, exactly(bytes));
            vec![one]
        };
        assert!(exactly(MAX_STRING_BYTES + 1).chars().count() <= MAX_STRING_BYTES);
        limit_is_sharp(with(MAX_STRING_BYTES), with(MAX_STRING_BYTES + 1), what);
    }
}

#[test]
fn an_absent_plan_and_label_are_within_the_limits() {
    let mut one = small();
    usage_mut(&mut one).plan = None;
    usage_mut(&mut one).account_label = None;
    assert!(within_limits(&[one]));
}

#[test]
fn a_very_long_label_is_over_the_limit() {
    // Under the 4 MiB of the body of a reply, and far over a label.
    let mut one = small();
    usage_mut(&mut one).windows[0].window = other("w", &"x".repeat(3 * 1024 * 1024));
    assert!(!within_limits(&[one]));
}

#[test]
fn the_list_of_hidden_metrics_is_limited_in_names_and_in_characters() {
    let names = |count: usize| {
        let mut one = small();
        one.metrics = (0..count).map(|index| format!("m{index}")).collect();
        vec![one]
    };
    limit_is_sharp(
        names(MAX_METRIC_NAMES),
        names(MAX_METRIC_NAMES + 1),
        "names",
    );

    let long = |characters: usize| {
        let mut one = small();
        // Four bytes each: 128 of them are far over the limit of the strings
        // in bytes, so the limit of a name is the one in characters.
        one.metrics = vec!["\u{1F600}".repeat(characters)];
        vec![one]
    };
    let ascii = {
        let mut one = small();
        one.metrics = vec!["a".repeat(MAX_METRIC_NAME_CHARACTERS + 1)];
        vec![one]
    };
    assert!(within_limits(&long(MAX_METRIC_NAME_CHARACTERS)));
    assert!(!within_limits(&long(MAX_METRIC_NAME_CHARACTERS + 1)));
    assert!(!within_limits(&ascii), "129 characters of one byte");
}

#[test]
fn the_number_of_partial_failures_of_a_snapshot_is_limited() {
    let failures = |count: usize| {
        let mut one = small();
        let data = usage_mut(&mut one).clone();
        one.usage = QueryOutcome::Partial {
            data,
            failures: vec![failure(); count],
        };
        vec![one]
    };
    limit_is_sharp(
        failures(MAX_PARTIAL_FAILURES),
        failures(MAX_PARTIAL_FAILURES + 1),
        "partial failures",
    );
}

/// Fills the snapshot with readings in windows of a kind with a fixed name,
/// each reading as long as the strings may be, so that the expanded size of
/// the snapshot is exactly `size`. The title of the snapshot counts as much
/// as it is long.
fn expanded(mut one: SnapshotPayload, size: u64) -> Vec<SnapshotPayload> {
    let data = usage_mut(&mut one);
    let title = (data.provider.as_str().len()
        + data.plan.as_deref().map_or(0, str::len)
        + one.account_id.len()) as u64;
    let mut left = size - title;
    let fixed = FIXED_WINDOW_NAME_BYTES;
    let mut readings = Vec::new();
    while left > 0 {
        let cost = left.min(MAX_STRING_BYTES as u64 + fixed);
        readings.push(reading(&"n".repeat((cost - fixed) as usize)));
        left -= cost;
    }
    usage_mut(&mut one).windows = readings
        .chunks(MAX_MEASUREMENTS_PER_WINDOW)
        .map(|chunk| window(UsageWindowKind::Monthly, chunk.to_vec()))
        .collect();
    vec![one]
}

fn titled(provider: usize, plan: Option<usize>, account_id: usize) -> SnapshotPayload {
    let mut one = snapshot(&"i".repeat(account_id), Vec::new());
    let data = usage_mut(&mut one);
    data.provider = ProviderId::new("p".repeat(provider));
    data.plan = plan.map(|bytes| "q".repeat(bytes));
    one
}

#[test]
fn the_expanded_size_of_a_reply_is_limited() {
    // A title of 1 + 1 bytes, and then one of the longest it may be.
    for (provider, plan, account_id) in [(1, None, 1), (256, Some(256), 256)] {
        let one = || titled(provider, plan, account_id);
        limit_is_sharp(
            expanded(one(), MAX_EXPANDED_BYTES),
            expanded(one(), MAX_EXPANDED_BYTES + 1),
            "expanded size",
        );
    }
}

#[test]
fn a_reply_whose_parts_are_each_within_a_limit_may_be_over_the_expanded_size() {
    // The window name of a reading is the id and the label, and it counts
    // for every reading of the window.
    let name = "n".repeat(MAX_STRING_BYTES);
    // Windows of 32 readings: half of the limit of a window.
    let per_window = MAX_MEASUREMENTS_PER_WINDOW / 2;
    let (id, label) = ("i".repeat(MAX_STRING_BYTES), "l".repeat(MAX_STRING_BYTES));
    let reply = |windows: usize| {
        let windows = (0..windows)
            .map(|_| window(other(&id, &label), vec![reading(&name); per_window]))
            .collect();
        vec![snapshot("a1", windows)]
    };
    // Each reading is 768 bytes, so 85 of them are the 64 KiB: two windows
    // (64 readings) are under it and three (96) are over it, with every count
    // and every string within its own limit.
    assert!(per_window as u64 * 2 * 768 <= MAX_EXPANDED_BYTES);
    assert!(per_window as u64 * 3 * 768 > MAX_EXPANDED_BYTES);
    assert!(within_limits(&reply(2)));
    assert!(!within_limits(&reply(3)));
}

/// A reply of the largest size in the two counts that the card model cares
/// for: `MAX_SNAPSHOTS` snapshots and `MAX_MEASUREMENTS` readings (one window
/// of eight in each snapshot), with an expanded size under its limit. The
/// other counts are not reached. The names of the readings share a
/// long prefix of many segments, which is the worst case for the card model:
/// it drops one segment of the prefix at a time, and copies all names each
/// time.
pub(crate) fn full_reply() -> Vec<SnapshotPayload> {
    let readings_each = MAX_MEASUREMENTS / MAX_SNAPSHOTS;
    // The title of a snapshot is `provider` and `account_id`, 1 and 3 bytes.
    let title = 4;
    let per_reading = (MAX_EXPANDED_BYTES as usize - MAX_SNAPSHOTS * title) / MAX_MEASUREMENTS;
    // Half of the bytes of a reading go to the name of its window and half to
    // its own name, in segments of two bytes.
    let prefix = "a-".repeat(per_reading / 2 / 2 - 1);
    let last_segments = ["x", "y", "z", "u", "v", "w", "s", "t"];
    assert_eq!(last_segments.len(), readings_each);
    (0..MAX_SNAPSHOTS)
        .map(|index| {
            let names = last_segments
                .iter()
                .map(|last| reading(&format!("{prefix}{last}")));
            let mut one = snapshot(
                &format!("{index:03}"),
                vec![window(other("", &prefix), names.collect())],
            );
            let data = usage_mut(&mut one);
            data.provider = ProviderId::new("p");
            data.plan = None;
            data.account_label = None;
            one
        })
        .collect()
}

#[test]
fn the_largest_reply_is_within_the_limits_and_makes_every_card() {
    let reply = full_reply();
    assert_eq!(reply.len(), MAX_SNAPSHOTS);
    assert!(within_limits(&reply));
    let readings: usize = reply
        .iter()
        .map(|each| match &each.usage {
            QueryOutcome::Complete { data } | QueryOutcome::Partial { data, .. } => data
                .windows
                .iter()
                .map(|w| w.measurements.len())
                .sum::<usize>(),
        })
        .sum();
    assert_eq!(readings, MAX_MEASUREMENTS);

    let cards = usage_cards(&reply, now());
    assert_eq!(cards.len(), 64);
    let rows: usize = cards.iter().map(|card| card.rows.len()).sum();
    assert_eq!(rows, 512);
}

#[test]
fn the_remote_daemon_doc_tells_the_limits_that_the_code_has() {
    let words = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let told = format!(
        "a reply with more than {MAX_SNAPSHOTS} accounts, more than {MAX_WINDOWS} windows in an \
         account, more than {MAX_MEASUREMENTS_PER_WINDOW} readings in a window or more than \
         {MAX_MEASUREMENTS} readings in all, more than {MAX_METRIC_NAMES} hidden metric names or \
         {MAX_PARTIAL_FAILURES} partial failures in an account, a hidden metric name of more \
         than {MAX_METRIC_NAME_CHARACTERS} characters, an account id, provider, plan, \
         label, window, reading or unit text of more than {MAX_STRING_BYTES} bytes, or more than {} KiB of text",
        MAX_EXPANDED_BYTES / 1024
    );
    assert!(words(include_str!("../../../docs/remote-daemon.md")).contains(&told));
}
