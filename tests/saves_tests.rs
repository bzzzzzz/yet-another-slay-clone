extern crate chrono;
extern crate tempfile;
extern crate yasc;

use std::cmp::Ordering;

use chrono::prelude::*;
use chrono::Duration;

use yasc::game::test_util::create_valid_engine;
use yasc::saves::SavedGamesCatalog;

#[test]
fn check_saved_games_catalog_is_empty_in_beginning() {
    let dir = tempfile::tempdir().unwrap();
    let catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    assert_eq!(catalog.list_saved_games().len(), 0);
}

#[test]
fn check_saved_games_catalog_is_not_empty_after_saving() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    assert_eq!(catalog.list_saved_games().len(), 0);

    let (_, _, engine) = create_valid_engine();
    let info = catalog.save("some_name", &engine);

    assert!(info.is_ok());
    assert_eq!(catalog.list_saved_games().len(), 1);

    let info = info.unwrap();
    assert_eq!(info.name, "some_name");
    assert_eq!(info.version, 1);
    let now = Utc::now();
    let before = now - Duration::seconds(10);
    assert_eq!(info.timestamp.cmp(&now), Ordering::Less);
    assert_eq!(info.timestamp.cmp(&before), Ordering::Greater);
}

#[test]
fn check_saved_engine_is_recoverable() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    let (_, _, engine) = create_valid_engine();
    let info = catalog.save("name", &engine);

    assert!(info.is_ok());

    let loaded_engine = catalog.load(&info.unwrap());
    assert!(loaded_engine.is_ok());
    assert_eq!(loaded_engine.unwrap(), engine);
}

#[test]
fn check_saved_engine_is_recoverable_through_new_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let mut catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();

    let (_, _, engine) = create_valid_engine();
    let info = catalog.save("name", &engine);

    assert!(info.is_ok());
    let info = info.unwrap();

    let other_catalog = SavedGamesCatalog::new(dir.path().to_str().unwrap(), "test").unwrap();
    assert_eq!(other_catalog.list_saved_games().len(), 1);
    assert_eq!(
        other_catalog.list_saved_games().get(0).cloned(),
        Some(info.clone())
    );

    let loaded_engine = other_catalog.load(&info);
    assert!(loaded_engine.is_ok());
    assert_eq!(loaded_engine.unwrap(), engine);
}
