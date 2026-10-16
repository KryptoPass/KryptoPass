use super::DemoStore;
use serde_json::Value;

fn loaded_store() -> Result<DemoStore, Box<dyn std::error::Error>> {
    let mut store = DemoStore::default();
    store.load()?;
    Ok(store)
}

fn fixture_entries() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("data.json"))?;
    Ok(fixture["entries"]
        .as_array()
        .ok_or("the demo fixture must contain entries")?
        .clone())
}

fn login_entries() -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    Ok(fixture_entries()?
        .into_iter()
        .filter(|entry| entry["kind"] == "login")
        .collect())
}

fn detail(store: &DemoStore) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(
        &store
            .detail()
            .ok_or("a selected unlocked entry has detail")?,
    )?)
}

fn assert_password_hidden(store: &DemoStore) -> Result<(), Box<dyn std::error::Error>> {
    let projected = detail(store)?;
    assert_eq!(projected["content"]["fields"]["password"], "");
    assert_eq!(projected["content"]["fields"]["passwordRevealed"], false);
    Ok(())
}

#[test]
fn entry_metadata_never_projects_content_or_password() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = loaded_store()?;
    let fixture = fixture_entries()?;
    let expected_keys = [
        "favorite",
        "folder",
        "icon",
        "id",
        "kind",
        "recipients",
        "relatedEntryId",
        "revision",
        "title",
        "trashed",
    ];
    let metadata = store.entries();
    assert_eq!(metadata.len(), fixture.len());

    for record in &metadata {
        let entry: Value = serde_json::from_str(record)?;
        let object = entry.as_object().ok_or("metadata must be an object")?;
        assert_eq!(object.len(), expected_keys.len());
        for key in expected_keys {
            assert!(object.contains_key(key), "metadata is missing {key}");
        }
        for original in &fixture {
            for sensitive_value in [
                original["value"].as_str(),
                original["details"]["password"].as_str(),
            ]
            .into_iter()
            .flatten()
            .filter(|value| !value.is_empty())
            {
                assert!(
                    !record.contains(sensitive_value),
                    "metadata leaked fixture content"
                );
            }
        }
    }

    let login = login_entries()?
        .into_iter()
        .next()
        .ok_or("the fixture needs a login entry")?;
    store.select(login["id"].as_str().ok_or("a login has an ID")?);
    store.toggle_password();
    assert_eq!(store.entries(), metadata);
    store.set_locked(true);
    assert_eq!(store.entries(), metadata);
    Ok(())
}

#[test]
fn selecting_a_login_starts_with_its_password_hidden() -> Result<(), Box<dyn std::error::Error>> {
    let mut store = loaded_store()?;
    assert!(store.detail().is_none());
    let login = login_entries()?
        .into_iter()
        .next()
        .ok_or("the fixture needs a login entry")?;
    store.select(login["id"].as_str().ok_or("a login has an ID")?);
    let projected = detail(&store)?;
    assert_eq!(projected["id"], login["id"]);
    assert_eq!(
        projected["content"]["fields"]["username"],
        login["details"]["username"]
    );
    assert_password_hidden(&store)?;
    store.toggle_password();
    let revealed = detail(&store)?;
    assert_eq!(
        revealed["content"]["fields"]["password"],
        login["details"]["password"]
    );
    assert_eq!(revealed["content"]["fields"]["passwordRevealed"], true);
    Ok(())
}

#[test]
fn locking_removes_detail_and_unlocking_keeps_password_hidden()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = loaded_store()?;
    let login = login_entries()?
        .into_iter()
        .next()
        .ok_or("the fixture needs a login entry")?;
    store.select(login["id"].as_str().ok_or("a login has an ID")?);
    store.toggle_password();
    assert_eq!(
        detail(&store)?["content"]["fields"]["passwordRevealed"],
        true
    );

    store.set_locked(true);
    assert!(store.detail().is_none());
    store.toggle_password();
    assert!(store.detail().is_none());

    store.set_locked(false);
    assert_eq!(detail(&store)?["id"], login["id"]);
    assert_password_hidden(&store)?;
    Ok(())
}

#[test]
fn changing_selection_discards_the_previous_password_revelation()
-> Result<(), Box<dyn std::error::Error>> {
    let mut store = loaded_store()?;
    let logins = login_entries()?;
    assert!(logins.len() >= 2, "the fixture needs two login entries");
    let first_id = logins[0]["id"].as_str().ok_or("a login has an ID")?;
    let second_id = logins[1]["id"].as_str().ok_or("a login has an ID")?;
    store.select(first_id);
    store.toggle_password();

    store.select(second_id);
    assert_eq!(detail(&store)?["id"], second_id);
    assert_password_hidden(&store)?;
    store.select(first_id);
    assert_password_hidden(&store)?;
    Ok(())
}

#[test]
fn selecting_an_unknown_entry_removes_the_previous_detail() -> Result<(), Box<dyn std::error::Error>>
{
    let mut store = loaded_store()?;
    let login = login_entries()?
        .into_iter()
        .next()
        .ok_or("the fixture needs a login entry")?;
    store.select(login["id"].as_str().ok_or("a login has an ID")?);
    store.toggle_password();
    store.select("unknown-demo-entry");
    assert!(store.detail().is_none());
    Ok(())
}
