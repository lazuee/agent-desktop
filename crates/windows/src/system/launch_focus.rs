//! Keeps a launch without `--activate` from leaving the launched application
//! in the foreground.
//!
//! Two lines of defence. The process starts with a show-window hint that asks
//! for no activation; an application that ignores the hint and activates
//! itself is then given the foreground back, but only when the foreground
//! window belongs to the launched process or to a process it started, and the
//! window that had it before the spawn is still open. A window the user
//! switched to is never touched.
//!
//! The watch runs beside the window observation on one horizon, so it never
//! spends the time the observation needs and never outlives the launch by more
//! than the foreground horizon. The observation reports through [`Progress`]
//! when the launch can no longer take the foreground, which ends the watch
//! early instead of leaving an exited application to burn the horizon.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use agent_desktop_core::{Deadline, InteractionLease, ProcessId, WindowFilter};

use super::app_ops::{ProcessRow, process_snapshot};
use super::process_identity::{ProcessIdentity, wait_for_exit};
use super::window::window_activate::focus_window;
use super::window::window_ops::{list_windows_live, window_id};

const STARTF_USESHOWWINDOW: u32 = 0x1;
const SW_SHOWNOACTIVATE: u16 = 4;
const FOCUS_WAIT: Duration = Duration::from_millis(3_000);
const SETTLE_GRACE: Duration = Duration::from_millis(150);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The window that owns the foreground and the application process behind it:
/// the hosted application for a frame-hosted window, as window listing reports
/// it (A26-8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Foreground {
    pub handle: isize,
    pub pid: u32,
}

/// The `dwFlags` and `wShowWindow` fields of the child's `STARTUPINFOW`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StartupShow {
    pub flags: u32,
    pub show: u16,
}

#[derive(Debug, PartialEq, Eq)]
enum Look {
    Stolen,
    Waiting,
    Unreadable,
    Elsewhere,
}

/// What the window observation tells the focus watch while both run.
#[derive(Debug, Default)]
pub(super) struct Progress {
    hopeless: AtomicBool,
}

impl Progress {
    /// A progress no watch reads, for a launch that does not watch the
    /// foreground.
    pub(super) fn new() -> Self {
        Self::default()
    }

    /// The launch is over, so nothing can take the foreground as the launch.
    pub(super) fn over(&self) {
        self.hopeless.store(true, Ordering::Relaxed);
    }

    /// The observation ended with no window. That is hopeless only once the
    /// launched process and the processes it started are gone: while any of
    /// them runs, it could still open the window that takes the foreground.
    pub(super) fn finished_without_window(&self, launched: ProcessId) {
        if launch_finished(launched) {
            self.over();
        }
    }

    fn hopeless(&self) -> bool {
        self.hopeless.load(Ordering::Relaxed)
    }
}

/// The focus watch of one launch, run beside the window observation instead of
/// ahead of it. The watch owns the lease and releases it when it ends; an
/// activating launch keeps its own lease and never builds a watch.
pub(super) struct Watch {
    prior: Option<Foreground>,
    launched: ProcessId,
    lease: InteractionLease,
}

impl Watch {
    pub(super) fn new(
        prior: Option<Foreground>,
        launched: ProcessId,
        lease: InteractionLease,
    ) -> Self {
        Self {
            prior,
            launched,
            lease,
        }
    }

    /// Runs the watch on its own thread while `observe` runs on this one and
    /// returns the watch's answer with the observation's. The watch and the
    /// restore inside it share one deadline, the foreground horizon capped to
    /// the lease, so neither side outlives the launch on its own budget.
    pub(super) fn beside<T>(
        self,
        window_is_open: impl Fn() -> bool + Send,
        observe: impl FnOnce(&Progress) -> T,
    ) -> (bool, T) {
        let progress = Progress::new();
        let Some(prior) = self.prior else {
            drop(self.lease);
            return (false, observe(&progress));
        };
        let Watch {
            launched, lease, ..
        } = self;
        let watching = Watching {
            prior,
            launched,
            wait: lease.deadline().capped(FOCUS_WAIT),
            lease,
            progress: &progress,
            window_is_open,
        };
        std::thread::scope(|scope| {
            let watching = scope.spawn(move || give_back(watching));
            let observed = observe(&progress);
            (watching.join().unwrap_or(false), observed)
        })
    }
}

struct Watching<'a, F> {
    prior: Foreground,
    launched: ProcessId,
    lease: InteractionLease,
    wait: Deadline,
    progress: &'a Progress,
    window_is_open: F,
}

pub(super) fn startup_show(activate: bool) -> StartupShow {
    if activate {
        return StartupShow { flags: 0, show: 0 };
    }
    StartupShow {
        flags: STARTF_USESHOWWINDOW,
        show: SW_SHOWNOACTIVATE,
    }
}

fn judge(prior: Foreground, now: Option<Foreground>, belongs: impl Fn(u32) -> bool) -> Look {
    match now {
        None => Look::Unreadable,
        Some(now) if now.handle == prior.handle => Look::Waiting,
        Some(now) if belongs(now.pid) => Look::Stolen,
        Some(_) => Look::Elsewhere,
    }
}

/// The verdict the watch reaches over its polls. A poll that names a launched
/// window hands over to the restore, a poll that names a readable foreign
/// window ends the watch, and an unreadable foreground says nothing, so the
/// watch keeps polling. `give_up` ends the watch at the horizon.
fn watch_foreground(
    prior: Foreground,
    mut read: impl FnMut() -> Option<Foreground>,
    mut give_up: impl FnMut() -> bool,
    belongs: impl Fn(u32) -> bool,
) -> Look {
    loop {
        let verdict = judge(prior, read(), &belongs);
        if verdict != Look::Waiting && verdict != Look::Unreadable {
            return verdict;
        }
        if give_up() {
            return Look::Waiting;
        }
    }
}

/// True when `pid` is `launched` or was started by it, directly or through
/// other processes that all pass `not_older`. Follows at most `rows.len() + 1`
/// links, so a parent cycle left by reused pids ends with false.
fn started_by(
    launched: u32,
    pid: u32,
    rows: &[ProcessRow],
    not_older: impl Fn(u32) -> bool,
) -> bool {
    let mut current = pid;
    for _ in 0..=rows.len() {
        if current == launched {
            return true;
        }
        let Some(row) = rows.iter().find(|row| row.pid.get() == current) else {
            return false;
        };
        if !not_older(current) {
            return false;
        }
        current = row.parent;
    }
    false
}

/// True when the launched process and every process it started are gone, so no
/// window can open that the launch owns. A process whose liveness cannot be
/// confirmed counts as running, so this never ends the watch on a guess.
fn launch_finished(launched: ProcessId) -> bool {
    if !wait_for_exit(launched, Duration::ZERO) {
        return false;
    }
    let Ok(rows) = process_snapshot() else {
        return false;
    };
    !rows
        .iter()
        .any(|row| started_by(launched.get(), row.pid.get(), &rows, |_| true))
}

/// Whether `pid` belongs to the launch: the launched process, or a process it
/// started through processes that are all no older than it. A parent pid
/// outlives its process, so an older process whose parent's pid the launch
/// reused is never counted, and neither is one whose start time cannot be
/// read. `launch` is none when the launched process had already exited.
fn launch_owns(launched: ProcessId, launch: Option<ProcessIdentity>, pid: u32) -> bool {
    if pid == launched.get() {
        return true;
    }
    let Some(launch) = launch else {
        return false;
    };
    let rows = process_snapshot().unwrap_or_default();
    started_by(launched.get(), pid, &rows, |pid| {
        ProcessIdentity::capture(ProcessId::new(pid))
            .ok()
            .flatten()
            .is_some_and(|process| !process.started_before(launch))
    })
}

/// Waits for the launched process to take the foreground and, when it does,
/// focuses the window that had it, both inside the same `wait` deadline. The
/// wait ends when a readable foreground window belongs to someone else, when
/// the launch's window has been open for the grace period without taking it,
/// when the observation reports the launch over, or at the deadline. True only
/// when the previous window reads back as the foreground afterwards.
fn give_back<F: Fn() -> bool>(watching: Watching<'_, F>) -> bool {
    let Watching {
        prior,
        launched,
        lease,
        wait,
        progress,
        window_is_open,
    } = watching;
    let launch = ProcessIdentity::capture(launched).ok().flatten();
    let mut opened: Option<Instant> = None;
    let verdict = watch_foreground(
        prior,
        read_foreground,
        || {
            if opened.is_none() && window_is_open() {
                opened = Some(Instant::now());
            }
            let settled = opened.is_some_and(|at| at.elapsed() >= SETTLE_GRACE);
            if progress.hopeless() || settled || wait.is_expired() {
                return true;
            }
            std::thread::sleep(POLL_INTERVAL.min(wait.remaining()));
            false
        },
        |pid| launch_owns(launched, launch, pid),
    );
    verdict == Look::Stolen && restore(prior, &lease, wait)
}

/// Focuses the window that had the foreground when a window of `owner` has
/// taken it since, as the prompt of an application asked to close can. A
/// foreground that did not change, or that went to another process, is left
/// alone.
pub(super) fn return_if_taken(
    prior: Foreground,
    owner: ProcessId,
    lease: &InteractionLease,
) -> bool {
    match judge(prior, read_foreground(), |pid| pid == owner.get()) {
        Look::Stolen => restore(prior, lease, lease.deadline()),
        Look::Waiting | Look::Unreadable | Look::Elsewhere => false,
    }
}

fn restore(prior: Foreground, lease: &InteractionLease, wait: Deadline) -> bool {
    let lease = lease.with_deadline(wait);
    let filter = WindowFilter {
        focused_only: false,
        app: None,
    };
    let Ok(windows) = list_windows_live(&filter, wait) else {
        return false;
    };
    let id = window_id(prior.handle as _);
    let Some(window) = windows
        .iter()
        .find(|window| window.id == id && window.pid == prior.pid)
    else {
        return false;
    };
    focus_window(window, &lease).is_ok() && read_foreground() == Some(prior)
}

/// The foreground window with the process an agent means by it: the hosted
/// application behind a frame host frame rather than the frame host, the same
/// identity window listing reports (A26-8). Reading the raw window owner here
/// would name `ApplicationFrameHost` for a hosted application, so a foreground
/// a launch took from such a window would never match the window listing gives
/// back, and the launch would be judged to own a window it does not.
#[cfg(target_os = "windows")]
pub(super) fn read_foreground() -> Option<Foreground> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    let handle = unsafe { GetForegroundWindow() };
    if handle.is_null() {
        return None;
    }
    let pid = crate::system::window::frame_identity::application_pid(handle)?;
    Some(Foreground {
        handle: handle as isize,
        pid: pid.get(),
    })
}

#[cfg(not(target_os = "windows"))]
pub(super) fn read_foreground() -> Option<Foreground> {
    None
}

#[cfg(test)]
#[path = "launch_focus_tests.rs"]
mod tests;
