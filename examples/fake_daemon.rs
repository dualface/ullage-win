//! A stand-in for the ullage daemon, to try the window by hand.
//!
//! It answers every request with the same canned snapshots. Windows only.
//!
//! ```text
//! $env:ULLAGE_CONTROL_PIPE = '\\.\pipe\ullage-fake'
//! cargo run --example fake_daemon -- rich      # or: empty, many
//! ullage-win.exe                               # in another shell, same variable
//! ```

#[cfg(not(windows))]
fn main() {
    eprintln!("the fake daemon runs on Windows only");
}

#[cfg(windows)]
fn main() {
    imp::run();
}

#[cfg(windows)]
mod imp {
    use chrono::{DateTime, Duration, Utc};
    use ullage_protocol::{
        CONTROL_PROTOCOL_VERSION, ControlRequest, ControlResponse, ControlResult, MeasurementUnit,
        ProviderId, QueryOutcome, SanitizedErrorPayload, SnapshotPayload, SubscriptionUsage,
        UsageMeasurement, UsageWindow, UsageWindowKind,
    };
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        FlushFileBuffers, PIPE_ACCESS_DUPLEX, ReadFile, WriteFile,
    };
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows::core::PCWSTR;

    fn window(kind: UsageWindowKind, used: f64, resets_in: i64, now: DateTime<Utc>) -> UsageWindow {
        UsageWindow {
            window: kind,
            resets_at: Some(now + Duration::seconds(resets_in)),
            measurements: vec![UsageMeasurement {
                name: "included_usage".into(),
                used,
                limit: Some(100.0),
                unit: MeasurementUnit::Percent,
            }],
        }
    }

    fn other(id: &str) -> UsageWindowKind {
        UsageWindowKind::Other {
            id: id.into(),
            label: id.into(),
        }
    }

    fn account(
        provider: &str,
        plan: &str,
        id: &str,
        stale: bool,
        windows: Vec<UsageWindow>,
        now: DateTime<Utc>,
    ) -> SnapshotPayload {
        SnapshotPayload {
            account_id: id.into(),
            usage: QueryOutcome::Complete {
                data: SubscriptionUsage {
                    provider: ProviderId::new(provider),
                    account_label: None,
                    plan: Some(plan.into()),
                    subscription_expires_at: None,
                    observed_at: now,
                    windows,
                },
            },
            last_success_at: now,
            stale,
            // The daemon marks a snapshot stale only with the error behind it.
            last_error: stale.then_some(SanitizedErrorPayload::Network),
            last_error_at: stale.then_some(now),
            metrics: Vec::new(),
        }
    }

    fn rich(now: DateTime<Utc>) -> Vec<SnapshotPayload> {
        const H: i64 = 3_600;
        const D: i64 = 24 * H;
        use UsageWindowKind::{FiveHours, Monthly, Weekly};
        vec![
            account(
                "chatgpt",
                "prolite",
                "account-7",
                false,
                vec![window(Weekly, 17.0, 6 * D + 2 * H, now)],
                now,
            ),
            account(
                "claude",
                "max_20x",
                "account-1",
                false,
                vec![
                    window(FiveHours, 21.0, 2 * H + 43 * 60, now),
                    window(Weekly, 27.0, 4 * D + 3 * H, now),
                    window(other("fable"), 0.0, 4 * D + 3 * H, now),
                ],
                now,
            ),
            account(
                "cursor",
                "Ultra",
                "account-5",
                false,
                vec![
                    window(other("auto"), 48.0, 9 * H + 29 * 60, now),
                    window(other("api"), 82.0, 9 * H + 29 * 60, now),
                    window(other("usage"), 48.0, 9 * H + 29 * 60, now),
                ],
                now,
            ),
            account(
                "devin",
                "Pro",
                "account-8",
                false,
                vec![
                    window(other("Daily"), 0.0, H + 53 * 60, now),
                    window(Weekly, 0.0, 6 * D + 20 * H, now),
                ],
                now,
            ),
            account(
                "grok",
                "SuperGrok Heavy",
                "account-4",
                true,
                vec![
                    window(Weekly, 28.0, 4 * D + 3 * H, now),
                    window(other("GrokBuild"), 100.0, 4 * D + 3 * H, now),
                ],
                now,
            ),
            account(
                "opencode",
                "Go",
                "account-9",
                false,
                vec![
                    window(FiveHours, 1.0, 3 * H + 58 * 60, now),
                    window(Monthly, 1.0, 22 * D + 3 * H, now),
                ],
                now,
            ),
        ]
    }

    fn read_request(pipe: HANDLE) -> Option<Vec<u8>> {
        let mut request = Vec::new();
        let mut chunk = [0_u8; 4096];
        loop {
            let mut read = 0;
            unsafe { ReadFile(pipe, Some(&mut chunk), Some(&mut read), None).ok()? };
            if read == 0 {
                return None;
            }
            request.extend_from_slice(&chunk[..read as usize]);
            if request.ends_with(b"\n") {
                return Some(request);
            }
        }
    }

    pub fn run() {
        let scenario = std::env::args().nth(1).unwrap_or_else(|| "rich".into());
        let name = std::env::var("ULLAGE_CONTROL_PIPE").expect("set ULLAGE_CONTROL_PIPE");
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        loop {
            let pipe = unsafe {
                CreateNamedPipeW(
                    PCWSTR(wide.as_ptr()),
                    PIPE_ACCESS_DUPLEX,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    PIPE_UNLIMITED_INSTANCES,
                    65536,
                    65536,
                    0,
                    None,
                )
            };
            // A pipe that is already taken is a daemon already running.
            if pipe.is_invalid() {
                eprintln!("cannot create the pipe {name}");
                return;
            }
            unsafe {
                let _ = ConnectNamedPipe(pipe, None);
            }
            if let Some(bytes) = read_request(pipe)
                && let Ok(request) = serde_json::from_slice::<ControlRequest>(&bytes)
            {
                let snapshots = match scenario.as_str() {
                    "empty" => Vec::new(),
                    // Enough accounts to fill a tall screen.
                    "many" => (0..4)
                        .flat_map(|copy| {
                            rich(Utc::now()).into_iter().map(move |mut snapshot| {
                                snapshot.account_id = format!("{}-{copy}", snapshot.account_id);
                                snapshot
                            })
                        })
                        .collect(),
                    _ => rich(Utc::now()),
                };
                let mut response =
                    ControlResponse::new(&request.request_id, ControlResult::Snapshots(snapshots));
                response.version = CONTROL_PROTOCOL_VERSION;
                response.daemon_version = Some("0.3.5".into());
                let mut encoded = serde_json::to_vec(&response).expect("a response encodes");
                encoded.push(b'\n');
                unsafe {
                    let _ = WriteFile(pipe, Some(&encoded), None, None);
                    let _ = FlushFileBuffers(pipe);
                }
            }
            unsafe {
                let _ = DisconnectNamedPipe(pipe);
                let _ = CloseHandle(pipe);
            }
        }
    }
}
