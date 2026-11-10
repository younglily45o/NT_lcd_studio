use lcd_studio::{
    Gamma, Project,
    c_format::{export_c, import_c},
    project::{MAX_FILE_SIZE, load_project, save_atomic},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct TestDir(PathBuf);
#[test]
fn c_import_rejects_ambiguous_leading_zero_literals() {
    let source = export_c(&Gamma::identity());
    for number in ["010", "08", "00"] {
        assert!(
            import_c(&source.replacen("0x08", number, 1)).is_err(),
            "must reject {number}"
        );
    }
}
impl TestDir {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "lcd-studio-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&p).unwrap();
        Self(p)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn real_dc162_initializer_ignores_commented_alternate_tables() {
    let source = include_str!("../assets/dc162-gamma-source.c");
    assert_eq!(import_c(source).unwrap(), Gamma::dc162());
}
#[test]
fn c_export_roundtrip_keeps_disabled_and_all_points() {
    let mut gamma = Gamma::dc162();
    gamma.enabled = false;
    gamma.blue[17] = 123;
    assert_eq!(import_c(&export_c(&gamma)).unwrap(), gamma);
}
#[test]
fn c_import_rejects_ambiguous_or_unsupported_code() {
    let good = export_c(&Gamma::identity());
    assert!(import_c(&format!("{good}\n{good}")).is_err());
    assert!(import_c(&good.replace(".enable = 1", ".enable = 1 + 0")).is_err());
    assert!(import_c(&good.replace(".enable = 1", "#if FLAG\n.enable = 1\n#endif")).is_err());
    assert!(import_c(&good.replace("0x00", "0x100")).is_err());
    assert!(import_c(&good.replace(".gamma_r", ".unknown")).is_err());
    assert!(import_c(&format!("{good}\n/* unterminated")).is_err());
    assert!(import_c("const char *s = \"static gamma_t gamma_config = {0};\";").is_err());
}
#[test]
fn json_roundtrip_keeps_baseline_separate_from_edit() {
    let mut project = Project::dc162();
    project.current.blue[16] = 64;
    let parsed = Project::from_json(&project.to_json().unwrap()).unwrap();
    assert_eq!(parsed, project);
    assert_eq!(parsed.baseline.blue[16], 128);
}
#[test]
fn json_rejects_unknown_versions_invalid_lengths_and_values() {
    let project = Project::dc162();
    let good = project.to_json().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&good).unwrap();
    value["format_version"] = 2.into();
    assert!(Project::from_json(&value.to_string()).is_err());
    value["format_version"] = 1.into();
    value["current"]["blue"] = serde_json::json!([0, 1]);
    assert!(Project::from_json(&value.to_string()).is_err());
    for bad in [
        serde_json::json!(256),
        serde_json::json!(-1),
        serde_json::json!(1.5),
    ] {
        let mut value: serde_json::Value = serde_json::from_str(&good).unwrap();
        value["current"]["red"][0] = bad;
        assert!(Project::from_json(&value.to_string()).is_err());
    }
}
#[test]
fn atomic_save_replaces_existing_and_leaves_no_temp_files() {
    let dir = TestDir::new();
    let path = dir.0.join("方案.json");
    std::fs::write(&path, b"old").unwrap();
    save_atomic(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}
#[test]
fn failed_replace_preserves_existing_destination() {
    let dir = TestDir::new();
    let target = dir.0.join("existing");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("keep"), b"original").unwrap();
    assert!(save_atomic(&target, b"new").is_err());
    assert_eq!(std::fs::read(target.join("keep")).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 1);
}
#[test]
fn file_loading_rejects_oversized_and_supports_unicode_paths() {
    let dir = TestDir::new();
    let path = dir.0.join("调屏.json");
    save_atomic(&path, Project::dc162().to_json().unwrap().as_bytes()).unwrap();
    assert_eq!(load_project(&path).unwrap(), Project::dc162());
    std::fs::write(&path, vec![b' '; MAX_FILE_SIZE + 1]).unwrap();
    assert!(load_project(&path).is_err());
}
