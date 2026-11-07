//! `crash_point!("name")`: abort the process at a named point, for kill tests.
//!
//! Kill tests run a service with [`ENV_VAR`] naming one or more points
//! (comma-separated). When execution reaches an armed point the process
//! aborts at once, with no destructors and no flushing, as `kill -9` would
//! leave it. The test then restarts the service and checks the invariants
//! (docs 03 section 5.6).
//!
//! Without the `enabled` feature the macro expands to `()`, so release
//! builds carry no trace of it and never read the variable.

/// The environment variable that arms points: a comma-separated list of names.
pub const ENV_VAR: &str = "COUNTERACTIVE_CRASH_POINT";

/// Marks a point a kill test may crash the process at.
#[cfg(feature = "enabled")]
#[macro_export]
macro_rules! crash_point {
    ($name:literal) => {
        $crate::hit($name)
    };
}

/// Marks a point a kill test may crash the process at. Compiled out: the
/// `enabled` feature is off.
#[cfg(not(feature = "enabled"))]
#[macro_export]
macro_rules! crash_point {
    ($name:literal) => {
        ()
    };
}

/// Aborts if `name` is armed. Called through [`crash_point!`].
#[cfg(feature = "enabled")]
pub fn hit(name: &str) {
    if is_armed_in(armed(), name) {
        eprintln!("crash point `{name}` armed: aborting");
        std::process::abort();
    }
}

/// The armed list, read once: points sit on hot paths.
#[cfg(feature = "enabled")]
fn armed() -> &'static str {
    static ARMED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ARMED.get_or_init(|| std::env::var(ENV_VAR).unwrap_or_default())
}

/// Whether a comma-separated `list` names `name`. Spaces around names are ignored.
pub fn is_armed_in(list: &str, name: &str) -> bool {
    !name.is_empty() && list.split(',').any(|entry| entry.trim() == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_parsing() {
        assert!(is_armed_in("after_insert_pending", "after_insert_pending"));
        assert!(is_armed_in("a, b ,c", "b"));
        assert!(!is_armed_in("after_insert", "after_insert_pending"));
        assert!(!is_armed_in("", ""));
        assert!(!is_armed_in("a,,b", ""));
    }

    #[test]
    fn the_macro_is_an_expression() {
        let unit: () = crash_point!("never_armed_in_unit_tests");
        assert_eq!(unit, ());
    }

    /// Runs this test binary again with one point armed, and checks the
    /// child died by `SIGABRT` exactly at that point.
    #[cfg(all(feature = "enabled", unix))]
    #[test]
    fn an_armed_point_aborts_the_process() {
        use std::os::unix::process::ExitStatusExt as _;
        use std::process::Command;

        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "tests::child_reaches_a_point", "--nocapture", "--test-threads=1"])
            .env(ENV_VAR, "other, demo_point")
            .env("CRASH_POINT_CHILD", "1")
            .output()
            .unwrap();
        assert_eq!(output.status.signal(), Some(6), "child status: {:?}", output.status);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("crash point `demo_point` armed"));
        assert!(!String::from_utf8_lossy(&output.stdout).contains("after the point"));
    }

    /// The child half of the test above; does nothing in a normal run.
    #[cfg(all(feature = "enabled", unix))]
    #[test]
    fn child_reaches_a_point() {
        if std::env::var_os("CRASH_POINT_CHILD").is_none() {
            return;
        }
        crash_point!("not_armed");
        crash_point!("demo_point");
        println!("after the point");
    }
}
