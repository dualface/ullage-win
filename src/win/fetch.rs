//! The threads that ask the daemon, so the window never waits for it.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use ullage_cli::SystemClient;
use ullage_cli::usage_view::{UsageFetch, fetch_usage};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use super::credstore::CredStore;
use super::util::post;
use crate::remote::address::RemoteAddress;
use crate::remote::fetch::fetch_remote;
use crate::state::FailureKind;

/// Posted to the window when a result is waiting.
pub const WM_FETCH_DONE: u32 = WM_APP + 2;

pub type Outcome = Result<UsageFetch, FailureKind>;

/// Which daemon a fetch asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Local,
    Remote(RemoteAddress),
}

/// A result, with the generation of the connection it was asked of. A
/// result of an earlier generation answers a connection the user has left.
pub struct Delivery {
    pub generation: u32,
    pub outcome: Outcome,
}

pub struct Fetcher {
    results: Sender<Delivery>,
    target: isize,
}

impl Fetcher {
    pub fn spawn(hwnd: HWND) -> (Self, Receiver<Delivery>) {
        let (results, deliveries) = channel();
        (
            Self {
                results,
                // A raw handle value can cross threads; `HWND` itself cannot.
                target: hwnd.0 as isize,
            },
            deliveries,
        )
    }

    /// Starts one fetch on a thread of its own, so that a fetch of a
    /// connection the user left does not hold up the next one. Its result
    /// is sent with `generation`, and `WM_FETCH_DONE` is posted.
    pub fn request(&self, source: Source, generation: u32) {
        let results = self.results.clone();
        let target = self.target;
        thread::spawn(move || {
            let outcome = match source {
                // The client is made for every fetch, so a changed
                // environment is picked up.
                Source::Local => {
                    fetch_usage(&SystemClient::from_environment()).map_err(FailureKind::Fetch)
                }
                Source::Remote(address) => fetch_remote(&CredStore, &address),
            };
            if results
                .send(Delivery {
                    generation,
                    outcome,
                })
                .is_err()
            {
                return;
            }
            post(target, WM_FETCH_DONE);
        });
    }
}
