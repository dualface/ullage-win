//! The thread that pairs with a remote daemon, so the window never waits
//! for it.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::WM_APP;

use super::util::post;
use crate::remote::address::RemoteAddress;
use crate::remote::pairing::{PairCode, PairError, device_name, pair};
use crate::remote::store::Token;

/// Posted to the window when a pairing has ended.
pub const WM_PAIR_DONE: u32 = WM_APP + 3;

pub struct PairResult {
    pub address: RemoteAddress,
    /// The epoch the pairing was started in; see `PairingEpoch`.
    pub epoch: u32,
    pub outcome: Result<Token, PairError>,
}

pub struct Pairer {
    results: Sender<PairResult>,
    target: isize,
}

impl Pairer {
    pub fn spawn(hwnd: HWND) -> (Self, Receiver<PairResult>) {
        let (results, outcomes) = channel();
        (
            Self {
                results,
                // A raw handle value can cross threads; `HWND` itself cannot.
                target: hwnd.0 as isize,
            },
            outcomes,
        )
    }

    /// Starts one pairing. Its end is a `PairResult` and a `WM_PAIR_DONE`.
    pub fn start(&self, address: RemoteAddress, code: PairCode, epoch: u32) {
        let results = self.results.clone();
        let target = self.target;
        thread::spawn(move || {
            let name = device_name(std::env::var("COMPUTERNAME").ok().as_deref());
            let outcome = pair(&address, &code, &name);
            let ended = PairResult {
                address,
                epoch,
                outcome,
            };
            if results.send(ended).is_err() {
                return;
            }
            post(target, WM_PAIR_DONE);
        });
    }
}
