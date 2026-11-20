//! Where the window goes on the desktop. All numbers are physical pixels of
//! the virtual desktop.

/// A rectangle given by its edges, as Windows gives monitors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edges {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Edges {
    /// This area made `by` pixels larger on every side.
    pub fn grown(self, by: i32) -> Self {
        Self {
            left: self.left - by,
            top: self.top - by,
            right: self.right + by,
            bottom: self.bottom + by,
        }
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

const DEFAULT_MARGIN_DIP: i32 = 16;

fn scaled(dip: i32, dpi: u32) -> i32 {
    (i64::from(dip) * i64::from(dpi) / 96) as i32
}

/// The top right of a work area, 16 DIPs from its right and top edges.
pub fn default_position(work: Edges, window: (i32, i32), dpi: u32) -> (i32, i32) {
    let margin = scaled(DEFAULT_MARGIN_DIP, dpi);
    (work.right - window.0 - margin, work.top + margin)
}

/// Where to put the window at start: where it was saved if the middle of
/// its title bar is on a monitor, and the default place otherwise.
pub fn restore_position(
    saved: Option<(i32, i32)>,
    window: (i32, i32),
    monitors: &[Edges],
    primary_work: Edges,
    dpi: u32,
) -> (i32, i32) {
    if let Some((x, y)) = saved {
        let probe = (x + window.0 / 2, y + scaled(20, dpi));
        if monitors
            .iter()
            .any(|monitor| monitor.contains(probe.0, probe.1))
        {
            return (x, y);
        }
    }
    default_position(primary_work, window, dpi)
}

/// Whether the bitmap the window is drawn on must be made again: it has to
/// match the size of the window in pixels and the scale it is drawn at, and
/// the window may reach its new size by other means than a call that resizes
/// the bitmap.
pub fn surface_is_stale(
    surface: (i32, i32),
    surface_dpi: u32,
    window: (i32, i32),
    dpi: u32,
) -> bool {
    surface != (window.0.max(1), window.1.max(1)) || surface_dpi != dpi
}

/// Moves a window of `size` that starts at `(x, y)` just far enough to lie
/// inside `work`, when it fits at all.
pub fn keep_inside(work: Edges, (x, y): (i32, i32), size: (i32, i32)) -> (i32, i32) {
    let fit = |pos: i32, len: i32, low: i32, high: i32| {
        if len >= high - low || pos < low {
            low
        } else if pos + len > high {
            high - len
        } else {
            pos
        }
    };
    (
        fit(x, size.0, work.left, work.right),
        fit(y, size.1, work.top, work.bottom),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Edges = Edges {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    const SECOND: Edges = Edges {
        left: 1920,
        top: 0,
        right: 3840,
        bottom: 1080,
    };

    #[test]
    fn an_area_grows_on_every_side() {
        assert_eq!(
            PRIMARY.grown(10),
            Edges {
                left: -10,
                top: -10,
                right: 1930,
                bottom: 1050
            }
        );
    }

    #[test]
    fn the_default_is_the_top_right_of_the_work_area_16_dips_in() {
        assert_eq!(
            default_position(PRIMARY, (800, 500), 96),
            (1920 - 800 - 16, 16)
        );
        assert_eq!(
            default_position(PRIMARY, (1600, 500), 192),
            (1920 - 1600 - 32, 32)
        );
    }

    #[test]
    fn a_saved_place_on_a_monitor_is_kept() {
        let monitors = [PRIMARY, SECOND];
        assert_eq!(
            restore_position(Some((2000, 100)), (800, 500), &monitors, PRIMARY, 96),
            (2000, 100)
        );
        // A negative place on a monitor left of the primary.
        let left = Edges {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        assert_eq!(
            restore_position(Some((-1500, 50)), (800, 500), &[left, PRIMARY], PRIMARY, 96),
            (-1500, 50)
        );
    }

    #[test]
    fn a_saved_place_on_no_monitor_falls_back_to_the_default() {
        let expected = default_position(PRIMARY, (800, 500), 96);
        assert_eq!(
            restore_position(Some((5000, 100)), (800, 500), &[PRIMARY], PRIMARY, 96),
            expected
        );
        assert_eq!(
            restore_position(Some((100, -900)), (800, 500), &[PRIMARY], PRIMARY, 96),
            expected
        );
        assert_eq!(
            restore_position(None, (800, 500), &[PRIMARY], PRIMARY, 96),
            expected
        );
        // The monitor that held it was unplugged.
        assert_eq!(
            restore_position(Some((2000, 100)), (800, 500), &[PRIMARY], PRIMARY, 96),
            expected
        );
    }

    #[test]
    fn a_surface_is_stale_when_its_size_or_its_scale_differs_from_the_window() {
        assert!(!surface_is_stale((400, 300), 96, (400, 300), 96));
        assert!(surface_is_stale((400, 300), 96, (600, 450), 144));
        // The window already has the new size, but the scale changed.
        assert!(surface_is_stale((600, 450), 96, (600, 450), 144));
        // The scale is the same and the size is not.
        assert!(surface_is_stale((600, 450), 144, (600, 460), 144));
        // A window of no size still has a one pixel surface.
        assert!(!surface_is_stale((1, 1), 96, (0, 0), 96));
    }

    #[test]
    fn a_window_is_moved_just_enough_to_lie_inside_the_work_area() {
        assert_eq!(keep_inside(PRIMARY, (1500, 100), (800, 500)), (1120, 100));
        assert_eq!(keep_inside(PRIMARY, (-50, 100), (800, 500)), (0, 100));
        assert_eq!(keep_inside(PRIMARY, (100, 800), (800, 500)), (100, 540));
        assert_eq!(keep_inside(PRIMARY, (100, 100), (800, 500)), (100, 100));
        // Too big to fit: pinned to the top left.
        assert_eq!(keep_inside(PRIMARY, (100, 100), (2500, 1200)), (0, 0));
    }
}
