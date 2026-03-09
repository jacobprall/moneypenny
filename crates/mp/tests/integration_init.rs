//! Integration tests: `mp init` and resulting layout.

mod common;

use common::{CONFIG_NAME, init_project};

#[test]
fn init_creates_config_and_data_dir() {
    let (_temp, config_path) = init_project().unwrap();

    assert!(config_path.exists());
    let content = std::fs::read_to_string(&config_path).unwrap();
    assert!(content.contains("[agents]"));
    assert!(content.contains("data_dir") || content.contains("mp-data"));

    let data_dir = config_path.parent().unwrap().join("mp-data");
    assert!(
        data_dir.is_dir(),
        "data dir should exist: {}",
        data_dir.display()
    );
}

#[test]
fn init_creates_agent_db_and_metadata_db() {
    let (_temp, config_path) = init_project().unwrap();
    let base = config_path.parent().unwrap();
    let data_dir = base.join("mp-data");

    assert!(
        data_dir.join("main.db").exists(),
        "main agent DB should exist"
    );
    assert!(
        data_dir.join("metadata.db").exists(),
        "metadata DB should exist"
    );
}

#[test]
fn init_refuses_to_overwrite_existing_config() {
    let (temp, _config_path) = init_project().unwrap();

    let out = common::run_mp(["--config", CONFIG_NAME, "init"], Some(temp.path())).unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("already exists") || stderr.contains("re-initialize"));
}

#[test]
fn init_creates_models_directory() {
    let (_temp, config_path) = init_project().unwrap();
    let base = config_path.parent().unwrap();
    let models = base.join("mp-data").join("models");
    assert!(
        models.is_dir(),
        "models dir should exist: {}",
        models.display()
    );
}
