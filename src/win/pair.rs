//! The thread that pairs with a remote daemon, so the window never waits
//! for it.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::remote::address::RemoteAddress;
use crate::remote::pairing::{PairCode, PairError, device_name, pair};
use crate::remote::store::Token;

/// Posted to the window when a pairing has ended.
pub const WM_PAIR_DONE: u32 = WM_APP + 3;

pub struct PairResult {
    pub address: RemoteAddress,
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
    pub fn start(&self, address: RemoteAddress, code: PairCode) {
        let results = self.results.clone();
        let target = self.target;
        thread::spawn(move || {
            let name = device_name(std::env::var("COMPUTERNAME").ok().as_deref());
            let outcome = pair(&address, &code, &name);
            if results.send(PairResult { address, outcome }).is_err() {
                return;
            }
            unsafe {
                let _ = PostMessageW(
                    Some(HWND(target as *mut _)),
                    WM_PAIR_DONE,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        });
    }
}
