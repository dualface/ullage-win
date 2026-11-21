//! The size a reply from a remote daemon may have, after `fetch_usage` has
//! accepted it and before the card model sees it.
//!
//! `fetch_usage` checks the shape of a reply, not its size, and the reply
//! body is bounded only in bytes (4 MiB). The card model copies the display
//! name of a window for every reading of it, and the window rebuilds the
//! cards each time it draws, so the work of the window thread grows with the
//! length of the labels times the number of readings. A daemon of the user's
//! own never comes near these limits; a forged or altered reply may. A reply
//! over any limit is dropped as a whole: nothing of it is shown.
//!
//! The limits are at least four times the largest value the providers of
//! `ullage-cli` can produce. Their basis is in `kanban` card
//! `20261121-remote-reply-limits-task`.

use ullage_protocol::{
    MeasurementUnit, QueryOutcome, SnapshotPayload, UsageMeasurement, UsageWindow, UsageWindowKind,
};

/// Accounts in one reply. Real: one per account of the user.
pub const MAX_SNAPSHOTS: usize = 64;
/// Windows of one account. Real: at most 15 (`sub2api` writes 15 of its own).
pub const MAX_WINDOWS: usize = 64;
/// Readings of one window. Real: at most 6.
pub const MAX_MEASUREMENTS_PER_WINDOW: usize = 32;
/// Readings of the whole reply. Real: at most 68 for one `sub2api` account.
pub const MAX_MEASUREMENTS: usize = 512;
/// Bytes of one string of a reply, as UTF-8. Real: at most 32.
pub const MAX_STRING_BYTES: usize = 256;
/// Characters of one name in the list of hidden metrics: the limit of the
/// daemon itself (`MAX_METRIC_NAME_CHARACTERS`), which counts characters.
pub const MAX_METRIC_NAME_CHARACTERS: usize = 128;
/// Names in the list of hidden metrics of one account: the limit of the
/// daemon itself (`MAX_METRIC_NAMES`).
pub const MAX_METRIC_NAMES: usize = 64;
/// Partial failures of one account. The card model only counts them.
pub const MAX_PARTIAL_FAILURES: usize = 64;
/// What the card model copies, summed over the reply, in bytes. It is an
/// estimate that is never below the truth, see [`within_limits`]. Real: about
/// 4 KiB for the largest account.
pub const MAX_EXPANDED_BYTES: u64 = 64 * 1024;

/// What the card model copies for a window name: the three kinds with a fixed
/// name are counted at the length of the longest of them ("5h", "weekly",
/// "monthly" and the like are shorter).
const FIXED_WINDOW_NAME_BYTES: u64 = 8;

/// Whether `snapshots` are within every limit. It reads lengths and counts and
/// copies nothing, so its own cost is linear in the size of the reply, and it
/// stops at the first limit that is passed.
///
/// The expanded size is the sum of
/// - per reading: the length of its window's name plus the length of its own
///   name. The card model copies the name of the window into every reading
///   of it. The length of a window name is that of `id` plus `label` for a
///   window of kind `Other`, which is never below the display name the card
///   model makes of it (`window_display_name` is private to `ullage-core`).
/// - per snapshot: `provider`, `plan` and `account_id`, the strings of the
///   title of its card.
///
/// It is a figure to bound the work, not a measure of memory.
pub fn within_limits(snapshots: &[SnapshotPayload]) -> bool {
    if snapshots.len() > MAX_SNAPSHOTS {
        return false;
    }
    let mut readings = 0_usize;
    let mut expanded = 0_u64;
    for snapshot in snapshots {
        if !snapshot_within_limits(snapshot, &mut readings, &mut expanded) {
            return false;
        }
    }
    true
}

fn snapshot_within_limits(
    snapshot: &SnapshotPayload,
    readings: &mut usize,
    expanded: &mut u64,
) -> bool {
    let (usage, failures) = match &snapshot.usage {
        QueryOutcome::Complete { data } => (data, &[][..]),
        QueryOutcome::Partial { data, failures } => (data, failures.as_slice()),
    };
    if snapshot.metrics.len() > MAX_METRIC_NAMES
        || failures.len() > MAX_PARTIAL_FAILURES
        || usage.windows.len() > MAX_WINDOWS
        || snapshot
            .metrics
            .iter()
            .any(|name| name.chars().count() > MAX_METRIC_NAME_CHARACTERS)
        || !short(&snapshot.account_id)
        || !short(usage.provider.as_str())
        || !short_option(usage.account_label.as_deref())
        || !short_option(usage.plan.as_deref())
    {
        return false;
    }
    *expanded += len(usage.provider.as_str())
        + usage.plan.as_deref().map_or(0, len)
        + len(&snapshot.account_id);
    *expanded <= MAX_EXPANDED_BYTES
        && usage
            .windows
            .iter()
            .all(|window| window_within_limits(window, readings, expanded))
}

fn window_within_limits(window: &UsageWindow, readings: &mut usize, expanded: &mut u64) -> bool {
    if window.measurements.len() > MAX_MEASUREMENTS_PER_WINDOW {
        return false;
    }
    let name_bytes = match &window.window {
        UsageWindowKind::Other { id, label } => {
            if !short(id) || !short(label) {
                return false;
            }
            len(id) + len(label)
        }
        _ => FIXED_WINDOW_NAME_BYTES,
    };
    *readings += window.measurements.len();
    if *readings > MAX_MEASUREMENTS {
        return false;
    }
    for measurement in &window.measurements {
        if !measurement_within_limits(measurement) {
            return false;
        }
        *expanded += name_bytes + len(&measurement.name);
        if *expanded > MAX_EXPANDED_BYTES {
            return false;
        }
    }
    true
}

fn measurement_within_limits(measurement: &UsageMeasurement) -> bool {
    short(&measurement.name)
        && match &measurement.unit {
            MeasurementUnit::Currency { code } => short(code),
            MeasurementUnit::Other { id, label } => short(id) && short(label),
            MeasurementUnit::Requests
            | MeasurementUnit::Tokens
            | MeasurementUnit::Percent
            | MeasurementUnit::Credits => true,
        }
}

fn short(text: &str) -> bool {
    text.len() <= MAX_STRING_BYTES
}

fn short_option(text: Option<&str>) -> bool {
    text.is_none_or(short)
}

fn len(text: &str) -> u64 {
    // `usize` to `u64` does not lose anything on the targets of this crate.
    text.len() as u64
}

// `pub(crate)`: the Windows timing test of the scene builds the largest reply.
#[cfg(test)]
pub(crate) mod tests;
