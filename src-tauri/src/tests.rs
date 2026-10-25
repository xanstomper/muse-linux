// Agent-layer tests: persistent shell + git status + fs.
use crate::tools;

#[test]
fn term_persists_state() {
    let app = tauri::test::mock_app();
    let h = app.handle().clone();

    let r1 = crate::agent::term_exec(
        h.clone(),
        "export MUSE_PROBE=hello".into(),
        None,
        Some(4000),
        Some(false),
    )
    .unwrap();
    assert!(r1.alive, "shell died on first exec");

    let r2 = crate::agent::term_exec(
        h.clone(),
        "echo $MUSE_PROBE".into(),
        None,
        Some(4000),
        Some(false),
    )
    .unwrap();
    assert!(
        r2.output.contains("hello"),
        "state lost between calls: {}",
        r2.output
    );

    let r3 =
        crate::agent::term_exec(h.clone(), "exit 0".into(), None, Some(2000), Some(false)).unwrap();
    // after exit the shell should be reported dead (or respawned next call)
    assert!(!r3.alive || r3.output.contains("__MUSE_DONE_0__"));
}

#[test]
fn git_status_reads_repo() {
    let dir = std::env::temp_dir().join("muse-git-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&dir)
        .status()
        .unwrap();
    std::fs::write(dir.join("a.txt"), "x").unwrap();
    let s = crate::agent::git_status(dir.to_string_lossy().into()).unwrap();
    // fresh repo with no commits reports HEAD; either way dirty file must show
    assert!(
        s.dirty.iter().any(|l| l.contains("a.txt")),
        "dirty not detected"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tools_still_work() {
    // guard the v0.2 surface while we extend it
    let dir = std::env::temp_dir().join("muse-tools-test");
    let _ = std::fs::create_dir_all(&dir);
    std::fs::write(dir.join("t.txt"), "native").unwrap();
    let list = tools::fs_list(Some(dir.to_string_lossy().into())).unwrap();
    assert!(list.iter().any(|e| e.name == "t.txt"));
    let _ = std::fs::remove_dir_all(&dir);
}
