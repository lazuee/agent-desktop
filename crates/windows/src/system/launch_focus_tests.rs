use super::*;

const USER: Foreground = Foreground {
    handle: 10,
    pid: 100,
};
const LAUNCHED: u32 = 200;

fn launched_window() -> Foreground {
    Foreground {
        handle: 20,
        pid: LAUNCHED,
    }
}

fn row(pid: u32, parent: u32) -> ProcessRow {
    ProcessRow {
        pid: ProcessId::from(pid),
        name: "app.exe".into(),
        parent,
    }
}

#[test]
fn a_foreground_window_of_the_launched_process_is_stolen_focus() {
    assert_eq!(
        judge(USER, Some(launched_window()), |pid| pid == LAUNCHED),
        Look::Stolen
    );
}

#[test]
fn the_unchanged_foreground_keeps_waiting() {
    assert_eq!(judge(USER, Some(USER), |_| true), Look::Waiting);
}

#[test]
fn a_window_of_a_process_the_launch_started_is_stolen_focus() {
    let rows = [row(300, LAUNCHED), row(400, 300), row(500, 1)];
    let owns = |pid| started_by(LAUNCHED, pid, &rows, |_| true);
    let child = Foreground {
        handle: 20,
        pid: 300,
    };
    let grandchild = Foreground {
        handle: 21,
        pid: 400,
    };
    assert_eq!(judge(USER, Some(child), owns), Look::Stolen);
    assert_eq!(judge(USER, Some(grandchild), owns), Look::Stolen);
}

#[test]
fn a_window_of_an_unrelated_process_is_never_touched() {
    let rows = [row(300, LAUNCHED), row(500, 1)];
    let unrelated = Foreground {
        handle: 30,
        pid: 500,
    };
    let owns = |pid| started_by(LAUNCHED, pid, &rows, |_| true);
    assert_eq!(judge(USER, Some(unrelated), owns), Look::Elsewhere);
}

#[test]
fn an_unreadable_foreground_is_not_another_application_taking_it() {
    let rows = [row(300, LAUNCHED)];
    let owns = |pid| started_by(LAUNCHED, pid, &rows, |_| true);
    assert_eq!(judge(USER, None, owns), Look::Unreadable);
}

#[test]
fn an_unreadable_foreground_between_the_prior_and_the_launch_still_restores() {
    let reads = [Some(USER), None, Some(launched_window())];
    let looks = reads.len();
    let mut remaining = reads.into_iter();
    let mut asked = 0;
    let verdict = watch_foreground(
        USER,
        || remaining.next().unwrap_or(Some(USER)),
        || {
            asked += 1;
            asked >= looks
        },
        |pid| pid == LAUNCHED,
    );
    assert_eq!(
        verdict,
        Look::Stolen,
        "a window change that leaves the foreground unreadable must not end the watch"
    );
}

#[test]
fn a_readable_foreign_foreground_ends_the_watch() {
    let unrelated = Foreground {
        handle: 30,
        pid: 500,
    };
    assert_eq!(
        watch_foreground(USER, || Some(unrelated), || false, |_| false),
        Look::Elsewhere
    );
}

#[test]
fn an_observation_with_no_window_yet_keeps_a_running_launch_watched() {
    let progress = Progress::new();
    progress.finished_without_window(ProcessId::from(std::process::id()));
    assert!(
        !progress.hopeless(),
        "a process that is still running can still open the window that takes the foreground"
    );
}

#[test]
fn an_observation_with_no_window_ends_the_watch_once_the_launch_exited() {
    let mut child = std::process::Command::new("cmd")
        .args(["/C", "exit"])
        .spawn()
        .expect("cmd.exe exits at once");
    let exited = ProcessId::from(child.id());
    let _ = child.wait();
    let progress = Progress::new();
    progress.finished_without_window(exited);
    assert!(
        progress.hopeless(),
        "a launch whose processes are gone cannot take the foreground"
    );
}

#[test]
fn a_chain_through_a_process_older_than_the_launch_is_not_counted() {
    let rows = [row(300, LAUNCHED), row(400, 300)];
    let child_of_an_older_process = Foreground {
        handle: 22,
        pid: 400,
    };
    let owns = |pid| started_by(LAUNCHED, pid, &rows, |pid| pid != 300);
    assert_eq!(
        judge(USER, Some(child_of_an_older_process), owns),
        Look::Elsewhere
    );
}

#[test]
fn a_parent_cycle_ends_without_claiming_the_launch() {
    let rows = [row(300, 400), row(400, 300)];
    let now = Foreground {
        handle: 20,
        pid: 300,
    };
    let owns = |pid| started_by(LAUNCHED, pid, &rows, |_| true);
    assert_eq!(judge(USER, Some(now), owns), Look::Elsewhere);
}

#[test]
fn a_background_launch_asks_for_no_activation() {
    let show = startup_show(false);
    assert_eq!(show.flags, 0x1);
    assert_eq!(show.show, 4);
}

#[test]
fn an_activating_launch_keeps_the_default_startup_show() {
    assert_eq!(startup_show(true), StartupShow { flags: 0, show: 0 });
}
