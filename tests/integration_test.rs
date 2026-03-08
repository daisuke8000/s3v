use s3v::{App, Command, Event, Mode, S3Client, S3Path};

async fn create_localstack_client() -> S3Client {
    let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(aws_config::Region::new("us-east-1"))
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            "test", "test", None, None, "test",
        ))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .endpoint_url("http://localhost:4566")
        .force_path_style(true)
        .build();

    let client = aws_sdk_s3::Client::from_conf(s3_config);
    S3Client::new(client, "us-east-1".to_string())
}

#[tokio::test]
#[ignore]
async fn test_list_buckets() {
    let client = create_localstack_client().await;
    let items = client.list(&S3Path::root()).await.unwrap();

    assert!(!items.is_empty(), "Should have at least one bucket");
    assert!(
        items.iter().any(|item| item.name() == "test-bucket"),
        "Should find test-bucket"
    );
}

#[tokio::test]
#[ignore]
async fn test_list_objects_in_bucket() {
    let client = create_localstack_client().await;
    let path = S3Path::bucket("test-bucket");
    let items = client.list(&path).await.unwrap();

    // Should have README.md (file) and folder/ (folder)
    assert!(
        items.len() >= 2,
        "Should have at least 2 items, got {}",
        items.len()
    );

    let has_readme = items.iter().any(|item| item.name() == "README.md");
    let has_folder = items.iter().any(|item| item.name() == "folder/");

    assert!(has_readme, "Should find README.md");
    assert!(has_folder, "Should find folder/");
}

#[tokio::test]
#[ignore]
async fn test_list_objects_in_subfolder() {
    let client = create_localstack_client().await;
    let path = S3Path::with_prefix("test-bucket", "folder/");
    let items = client.list(&path).await.unwrap();

    let has_cargo_toml = items.iter().any(|item| item.name() == "Cargo.toml");
    let has_sub = items.iter().any(|item| item.name() == "sub/");

    assert!(has_cargo_toml, "Should find Cargo.toml in folder/");
    assert!(has_sub, "Should find sub/ subfolder");
}

#[tokio::test]
#[ignore]
async fn test_file_has_size() {
    let client = create_localstack_client().await;
    let path = S3Path::bucket("test-bucket");
    let items = client.list(&path).await.unwrap();

    let readme = items
        .iter()
        .find(|item| item.name() == "README.md")
        .expect("Should find README.md");

    assert!(
        readme.size().unwrap() > 0,
        "README.md should have non-zero size"
    );
}

#[tokio::test]
#[ignore]
async fn test_full_navigation_flow() {
    let client = create_localstack_client().await;

    // 1. Start at root - list buckets
    let app = App::new();
    let buckets = client.list(&app.current_path).await.unwrap();
    let (app, _) = app.handle_event(Event::ItemsLoaded(buckets));
    assert_eq!(app.mode, Mode::Normal);
    assert!(!app.items.is_empty());

    // 2. Find and enter test-bucket
    let bucket_idx = app
        .items
        .iter()
        .position(|item| item.name() == "test-bucket")
        .expect("Should find test-bucket");
    let app = App {
        cursor: bucket_idx,
        ..app
    };

    let (app, cmds) = app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Enter,
        crossterm::event::KeyModifiers::empty(),
    )));

    assert_eq!(app.mode, Mode::Loading);
    let load_path = cmds
        .iter()
        .find_map(|cmd| match cmd {
            Command::LoadItems(path) => Some(path.clone()),
            _ => None,
        })
        .expect("Expected LoadItems command");

    // 3. Load bucket contents
    let items = client.list(&load_path).await.unwrap();
    let (app, _) = app.handle_event(Event::ItemsLoaded(items));
    assert!(app.items.iter().any(|item| item.name() == "README.md"));
    assert!(app.items.iter().any(|item| item.name() == "folder/"));

    // 4. Go back to root
    let (app, cmds) = app.handle_event(Event::Key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Esc,
        crossterm::event::KeyModifiers::empty(),
    )));
    assert!(app.current_path.is_root());
    assert!(cmds.iter().any(|cmd| matches!(cmd, Command::LoadItems(_))));
}
