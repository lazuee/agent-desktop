use super::{blob_names_agent_desktop, environment_has_marker, process_start_time};

const MARKER: &str = "AGENT_DESKTOP_INTERNAL_COMMAND_HOST=1";

fn procargs(arguments: &[&str], environment: &[&str]) -> Vec<u8> {
    let mut buffer = (arguments.len() as i32).to_ne_bytes().to_vec();
    let mut path = b"/usr/local/bin/agent-desktop\0".to_vec();
    path.resize(path.len().next_multiple_of(8), 0);
    buffer.extend_from_slice(&path);
    for field in arguments.iter().chain(environment) {
        buffer.extend_from_slice(field.as_bytes());
        buffer.push(0);
    }
    buffer
}

#[test]
fn the_marker_counts_only_as_an_exact_environment_entry() {
    assert!(environment_has_marker(&procargs(
        &["agent-desktop", ""],
        &["HOME=/Users/a", MARKER]
    )));
    assert!(!environment_has_marker(&procargs(
        &["agent-desktop", MARKER],
        &["HOME=/Users/a"]
    )));
    for entry in [
        "AGENT_DESKTOP_INTERNAL_COMMAND_HOST=0",
        "AGENT_DESKTOP_INTERNAL_COMMAND_HOST=1suffix",
        "OTHER=AGENT_DESKTOP_INTERNAL_COMMAND_HOST=1",
    ] {
        assert!(!environment_has_marker(&procargs(
            &["agent-desktop"],
            &[entry]
        )));
    }
}

#[test]
fn an_empty_first_argument_keeps_the_environment_aligned() {
    assert!(environment_has_marker(&procargs(&["", "x"], &[MARKER])));
    assert!(!environment_has_marker(&procargs(
        &["", MARKER],
        &["HOME=/Users/a"]
    )));
}

#[test]
fn malformed_argument_blocks_fail_closed() {
    assert!(!environment_has_marker(&[1, 0]));
    for count in [0, -1, i32::MAX] {
        let mut buffer = procargs(&["agent-desktop"], &[MARKER]);
        buffer[..4].copy_from_slice(&count.to_ne_bytes());
        assert!(!environment_has_marker(&buffer));
    }
    let mut truncated = procargs(&["agent-desktop"], &[MARKER]);
    truncated.pop();
    assert!(!environment_has_marker(&truncated));
}

#[test]
fn process_start_time_requires_the_process_owner() {
    let uid = unsafe { libc::geteuid() };
    let pid = std::process::id() as libc::pid_t;
    assert!(process_start_time(pid, uid).is_some());
    assert!(process_start_time(pid, uid ^ 1).is_none());
    if uid != 0 {
        assert!(process_start_time(1, uid).is_none());
    }
}

fn identity_blob(identifier: &str, trailing: &[u8]) -> Vec<u8> {
    let mut blob = (identifier.len() as u64 + 1).to_be_bytes().to_vec();
    blob.extend_from_slice(identifier.as_bytes());
    blob.push(0);
    blob.extend_from_slice(trailing);
    blob
}

#[test]
fn only_an_agent_desktop_identity_names_agent_desktop() {
    assert!(blob_names_agent_desktop(&identity_blob(
        "agent_desktop",
        &[]
    )));
    assert!(blob_names_agent_desktop(&identity_blob(
        "agent_desktop-ffi",
        &[]
    )));
    for other in ["agent-desktop", "agent_desktopz", "com.apple.Terminal", ""] {
        assert!(
            !blob_names_agent_desktop(&identity_blob(other, &[])),
            "{other}"
        );
    }
}

#[test]
fn an_identity_blob_without_a_terminator_fails_closed() {
    let mut blob = identity_blob("agent_desktop", &[]);
    blob.pop();
    assert!(!blob_names_agent_desktop(&blob));
    assert!(!blob_names_agent_desktop(&(8u64).to_be_bytes()));
}

#[test]
fn the_padding_after_the_identifier_is_not_read_as_the_name() {
    assert!(blob_names_agent_desktop(&identity_blob(
        "agent_desktop",
        &[0; 400]
    )));
}
