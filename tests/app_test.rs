use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use s3v::{App, Command, Event, Mode, S3Item, S3Path};

fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

/// バナーを閉じた状態の App を作成するヘルパー
fn app_without_banner() -> App {
    let mut app = App::new();
    app.show_banner = false;
    app
}

#[test]
fn test_app_initial_state() {
    let app = App::new();
    assert!(app.current_path.is_root());
    assert!(app.items.is_empty());
    assert_eq!(app.cursor, 0);
    assert_eq!(app.mode, Mode::Loading);
    assert!(app.running);
    assert!(app.show_banner);
}

#[test]
fn test_app_items_loaded() {
    let app = App::new();
    let items = vec![
        S3Item::Bucket {
            name: "bucket-1".to_string(),
        },
        S3Item::Bucket {
            name: "bucket-2".to_string(),
        },
    ];

    let (app, cmd) = app.handle_event(Event::ItemsLoaded(items));
    assert_eq!(app.items.len(), 2);
    assert_eq!(app.cursor, 0);
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.show_banner, "Banner should remain after items loaded");
    assert!(cmd.is_none());
}

#[test]
fn test_app_banner_dismissed_by_keypress() {
    let app = App::new();
    assert!(app.show_banner, "Banner should show on startup");

    // 任意のキーでバナーを閉じる
    let (app, cmd) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert!(!app.show_banner, "Banner should hide after keypress");
    assert!(
        cmd.is_none(),
        "Dismissing banner should not produce a command"
    );
}

#[test]
fn test_app_cursor_movement() {
    let mut app = app_without_banner();
    app.items = vec![
        S3Item::Bucket {
            name: "bucket-1".to_string(),
        },
        S3Item::Bucket {
            name: "bucket-2".to_string(),
        },
        S3Item::Bucket {
            name: "bucket-3".to_string(),
        },
    ];
    app.mode = Mode::Normal;

    // Move down
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Down)));
    assert_eq!(app.cursor, 1);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('j'))));
    assert_eq!(app.cursor, 2);

    // Can't go below last item
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Down)));
    assert_eq!(app.cursor, 2);

    // Move up
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Up)));
    assert_eq!(app.cursor, 1);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('k'))));
    assert_eq!(app.cursor, 0);

    // Can't go above first item
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Up)));
    assert_eq!(app.cursor, 0);
}

#[test]
fn test_app_enter_bucket() {
    let mut app = app_without_banner();
    app.items = vec![S3Item::Bucket {
        name: "my-bucket".to_string(),
    }];
    app.mode = Mode::Normal;

    let (app, cmd) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));

    assert_eq!(app.current_path.bucket, Some("my-bucket".to_string()));
    assert_eq!(app.mode, Mode::Loading);

    match cmd {
        Some(Command::LoadItems(path)) => {
            assert_eq!(path.bucket, Some("my-bucket".to_string()));
        }
        _ => panic!("Expected LoadItems command"),
    }
}

#[test]
fn test_app_enter_folder() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.items = vec![S3Item::Folder {
        name: "folder/".to_string(),
        prefix: "folder/".to_string(),
    }];
    app.mode = Mode::Normal;

    let (app, cmd) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));

    assert_eq!(app.current_path.prefix, "folder/");
    assert_eq!(app.mode, Mode::Loading);

    match cmd {
        Some(Command::LoadItems(path)) => {
            assert_eq!(path.prefix, "folder/");
        }
        _ => panic!("Expected LoadItems command"),
    }
}

#[test]
fn test_app_go_back() {
    let mut app = app_without_banner();
    app.current_path = S3Path::with_prefix("my-bucket", "folder/subfolder/");
    app.mode = Mode::Normal;

    let (app, cmd) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));

    assert_eq!(app.current_path.prefix, "folder/");
    assert_eq!(app.mode, Mode::Loading);

    match cmd {
        Some(Command::LoadItems(path)) => {
            assert_eq!(path.prefix, "folder/");
        }
        _ => panic!("Expected LoadItems command"),
    }
}

#[test]
fn test_app_go_back_to_root() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.mode = Mode::Normal;

    let (app, cmd) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));

    assert!(app.current_path.is_root());

    match cmd {
        Some(Command::LoadItems(path)) => {
            assert!(path.is_root());
        }
        _ => panic!("Expected LoadItems command"),
    }
}

#[test]
fn test_app_quit() {
    let app = App::new();
    let (app, cmd) = app.handle_event(Event::Quit);

    assert!(!app.running);
    assert!(matches!(cmd, Some(Command::Quit)));
}
