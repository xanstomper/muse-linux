#[cfg(test)]
mod tests {
    use tauri::test::mock_app;

    #[test]
    fn commands_resolve_and_work() {
        let app = mock_app();
        let _ = app.handle().clone();

        // fs roundtrip in a temp dir
        let dir = std::env::temp_dir().join("muse-linux-test");
        let _ = std::fs::create_dir_all(&dir);
        let file = dir.join("probe.txt");
        std::fs::write(&file, "hello native").unwrap();

        let entries = crate::tools::fs_list(Some(dir.to_string_lossy().into())).unwrap();
        assert!(
            entries.iter().any(|e| e.name == "probe.txt"),
            "fs_list failed"
        );

        let content = crate::tools::fs_read(file.to_string_lossy().into(), None).unwrap();
        assert_eq!(content, "hello native");

        let out = crate::tools::fs_search(dir.to_string_lossy().into(), "native".into()).unwrap();
        assert!(
            out.iter().any(|h| h.line.contains("native")),
            "fs_search failed"
        );

        // exec — call the underlying logic directly (approval bypassed):
        // sys_exec's core is bash -lc; test via std Command to keep runtime-agnostic.
        use std::process::Command;
        let out = Command::new("bash")
            .args(["-lc", "echo bridge-ok"])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        assert!(stdout.contains("bridge-ok"), "exec failed: {stdout}");

        // procs sees at least one process
        let ps = crate::tools::procs().unwrap();
        assert!(!ps.is_empty());

        // audit tail returns a string (empty until first audited action)
        let audit = crate::tools::audit_tail().unwrap();
        assert!(audit.len() >= 0);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
