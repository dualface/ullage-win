//! The thread that asks the daemon, so the window never waits for it.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use ullage_cli::SystemClient;
use ullage_cli::usage_view::{FetchError, UsageFetch, fetch_usage};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

/// Posted to the window when a result is waiting.
pub const WM_FETCH_DONE: u32 = WM_APP + 2;

pub type Outcome = Result<UsageFetch, FetchError>;

pub struct Fetcher {
    requests: Sender<()>,
}

impl Fetcher {
    /// Starts the thread. Each request makes one fetch, and its result is
    /// sent on the returned channel, with `WM_FETCH_DONE` posted to `hwnd`.
    pub fn spawn(hwnd: HWND) -> (Self, Receiver<Outcome>) {
        let (requests, wanted) = channel::<()>();
        let (results, outcomes) = channel::<Outcome>();
        // A raw handle value can cross threads; `HWND` itself cannot.
        let target = hwnd.0 as isize;
        thread::spawn(move || {
            while wanted.recv().is_ok() {
                // The client is made for every fetch, so a changed
                // environment is picked up.
                let outcome = fetch_usage(&SystemClient::from_environment());
                if results.send(outcome).is_err() {
                    return;
                }
                unsafe {
                    let _ = PostMessageW(
                        Some(HWND(target as *mut _)),
                        WM_FETCH_DONE,
                        WPARAM(0),
                        LPARAM(0),
                    );
                }
            }
        });
        (Self { requests }, outcomes)
    }

    pub fn request(&self) {
        let _ = self.requests.send(());
    }
}
