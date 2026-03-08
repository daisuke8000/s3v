use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use s3v::app::download;
use s3v::{App, Command, Event, Mode, S3Item, S3Path};

fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

/// バナーを閉じた状態の App を作成するヘルパー
fn app_without_banner() -> App {
    let mut app = App::new();
    app.banner_state = s3v::BannerState::Active;
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
    assert_eq!(app.banner_state, s3v::BannerState::Splash);
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

    let (app, cmds) = app.handle_event(Event::ItemsLoaded(items));
    assert_eq!(app.items.len(), 2);
    assert_eq!(app.cursor, 0);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        app.banner_state,
        s3v::BannerState::Active,
        "Banner should transition to Active after items loaded"
    );
    assert!(cmds.is_empty());
}

#[test]
fn test_app_banner_dismissed_by_keypress() {
    let app = App::new();
    assert_eq!(
        app.banner_state,
        s3v::BannerState::Splash,
        "Banner should show on startup"
    );

    // 任意のキーでバナーを閉じる
    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(
        app.banner_state,
        s3v::BannerState::Active,
        "Banner should transition to Active after keypress"
    );
    assert!(
        cmds.is_empty(),
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

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));

    assert_eq!(app.current_path.bucket, Some("my-bucket".to_string()));
    assert_eq!(app.mode, Mode::Loading);

    assert!(
        cmds.iter().any(|cmd| matches!(cmd, Command::LoadItems(path) if path.bucket == Some("my-bucket".to_string()))),
        "Expected LoadItems command for my-bucket"
    );
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

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));

    assert_eq!(app.current_path.prefix, "folder/");
    assert_eq!(app.mode, Mode::Loading);

    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::LoadItems(path) if path.prefix == "folder/")),
        "Expected LoadItems command for folder/"
    );
}

#[test]
fn test_app_go_back() {
    let mut app = app_without_banner();
    app.current_path = S3Path::with_prefix("my-bucket", "folder/subfolder/");
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));

    assert_eq!(app.current_path.prefix, "folder/");
    assert_eq!(app.mode, Mode::Loading);

    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::LoadItems(path) if path.prefix == "folder/")),
        "Expected LoadItems command for folder/"
    );
}

#[test]
fn test_app_go_back_to_root() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));

    assert!(app.current_path.is_root());

    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::LoadItems(path) if path.is_root())),
        "Expected LoadItems command for root"
    );
}

#[test]
fn test_app_quit() {
    let app = App::new();
    let (app, cmds) = app.handle_event(Event::Quit);

    assert!(!app.running);
    assert!(cmds.iter().any(|cmd| matches!(cmd, Command::Quit)));
}

#[test]
fn test_toggle_selection() {
    let mut app = app_without_banner();
    app.items = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.txt".into(),
            key: "b.txt".into(),
            size: 200,
            last_modified: None,
        },
    ];
    app.mode = Mode::Normal;

    // Select first item
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char(' '))));
    assert!(app.selected.contains(&0));

    // Toggle off
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char(' '))));
    assert!(!app.selected.contains(&0));
}

#[test]
fn test_select_all() {
    let mut app = app_without_banner();
    app.items = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.txt".into(),
            key: "b.txt".into(),
            size: 200,
            last_modified: None,
        },
    ];
    app.mode = Mode::Normal;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('a'))));
    assert_eq!(app.selected.len(), 2);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('a'))));
    assert!(app.selected.is_empty());
}

#[test]
fn test_h_goes_back() {
    let mut app = app_without_banner();
    app.current_path = S3Path::with_prefix("my-bucket", "folder/");
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Char('h'))));
    assert_eq!(app.current_path.bucket, Some("my-bucket".to_string()));
    assert_eq!(app.current_path.prefix, "");
    assert!(cmds.iter().any(|cmd| matches!(cmd, Command::LoadItems(_))));
}

#[test]
fn test_l_enters_item() {
    let mut app = app_without_banner();
    app.items = vec![S3Item::Bucket {
        name: "my-bucket".into(),
    }];
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Char('l'))));
    assert_eq!(app.current_path.bucket, Some("my-bucket".to_string()));
    assert!(cmds.iter().any(|cmd| matches!(cmd, Command::LoadItems(_))));
}

#[test]
fn test_filter_mode_entry() {
    let mut app = app_without_banner();
    app.items = vec![S3Item::File {
        name: "a.txt".into(),
        key: "a.txt".into(),
        size: 100,
        last_modified: None,
    }];
    app.mode = Mode::Normal;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('/'))));
    assert_eq!(app.mode, Mode::Filter);
}

#[test]
fn test_filter_applies() {
    let mut app = app_without_banner();
    app.items = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.json".into(),
            key: "b.json".into(),
            size: 200,
            last_modified: None,
        },
    ];
    app.mode = Mode::Normal;

    // Enter filter mode
    let (mut app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('/'))));
    // Type filter text (directly set for test simplicity)
    app.filter = "*.json".to_string();
    // Apply
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(app.items.len(), 1);
    assert_eq!(app.items[0].name(), "b.json");
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn test_filter_cancel() {
    let mut app = app_without_banner();
    app.items = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.json".into(),
            key: "b.json".into(),
            size: 200,
            last_modified: None,
        },
    ];
    app.mode = Mode::Normal;

    let (mut app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('/'))));
    app.filter = "*.json".to_string();
    // Cancel with Esc
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));
    assert_eq!(app.items.len(), 2); // unchanged
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn test_preview_mode_entry_for_text_file() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.items = vec![S3Item::File {
        name: "readme.md".into(),
        key: "readme.md".into(),
        size: 100,
        last_modified: None,
    }];
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(app.mode, Mode::Loading);
    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::LoadPreview { .. }))
    );
}

#[test]
fn test_preview_scroll() {
    let mut app = app_without_banner();
    app.mode = Mode::PreviewFocus;
    app.preview_content = Some(s3v::preview::PreviewContent::Text(
        "line1\nline2\nline3".into(),
    ));
    app.preview_scroll = 0;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('j'))));
    assert_eq!(app.preview_scroll, 1);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('k'))));
    assert_eq!(app.preview_scroll, 0);
}

#[test]
fn test_preview_close() {
    let mut app = app_without_banner();
    app.mode = Mode::PreviewFocus;
    app.preview_content = Some(s3v::preview::PreviewContent::Text("content".into()));

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn test_selection_cleared_on_navigation() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.items = vec![S3Item::Folder {
        name: "folder/".into(),
        prefix: "folder/".into(),
    }];
    app.mode = Mode::Normal;
    app.selected.insert(0);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert!(app.selected.is_empty());
}

#[test]
fn test_search_mode_entry() {
    let mut app = app_without_banner();
    app.mode = Mode::Normal;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('?'))));
    assert_eq!(app.mode, Mode::Search);
    assert!(app.search_query.is_empty());
}

#[test]
fn test_search_mode_cancel() {
    let mut app = app_without_banner();
    app.mode = Mode::Search;
    app.search_query = "name LIKE '%test%'".to_string();

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.search_query.is_empty());
}

#[test]
fn test_search_mode_execute() {
    let mut app = app_without_banner();
    app.mode = Mode::Search;
    app.search_query = "name LIKE '%test%'".to_string();

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(app.mode, Mode::Loading);
    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::ExecuteSearch(_)))
    );
}

#[test]
fn test_search_results_loaded() {
    let mut app = app_without_banner();
    app.mode = Mode::Loading;
    let results = vec![S3Item::File {
        name: "found.txt".into(),
        key: "found.txt".into(),
        size: 100,
        last_modified: None,
    }];
    let (app, _) = app.handle_event(Event::SearchResults(results));
    assert_eq!(app.items.len(), 1);
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn test_metadata_indexed() {
    let app = app_without_banner();
    let (app, _) = app.handle_event(Event::MetadataIndexed(42));
    assert!(app.metadata_indexed);
    assert_eq!(app.metadata_count, 42);
}

#[test]
fn test_search_rejects_semicolon() {
    let index = s3v::search::MetadataIndex::new().unwrap();
    let result = index.search("1=1; DROP TABLE objects");
    assert!(result.is_err());
}

#[test]
fn test_search_rejects_multiple_statements() {
    let index = s3v::search::MetadataIndex::new().unwrap();
    let result = index.search("1=1; SELECT * FROM objects");
    assert!(result.is_err());
}

#[test]
fn test_preview_chunk_appends_text() {
    let mut app = app_without_banner();
    app.mode = Mode::PreviewFocus;
    app.preview_content = Some(s3v::preview::PreviewContent::StreamingText {
        partial_text: "hello".into(),
        key: "test.txt".into(),
    });

    let (app, _) = app.handle_event(Event::PreviewChunk(" world".into()));
    match &app.preview_content {
        Some(s3v::preview::PreviewContent::StreamingText { partial_text, .. }) => {
            assert_eq!(partial_text, "hello world");
        }
        _ => panic!("Expected StreamingText"),
    }
}

#[test]
fn test_preview_stream_complete() {
    let mut app = app_without_banner();
    app.mode = Mode::PreviewFocus;
    app.preview_content = Some(s3v::preview::PreviewContent::StreamingText {
        partial_text: "raw text".into(),
        key: "test.txt".into(),
    });

    let (app, _) = app.handle_event(Event::PreviewStreamComplete(Some("formatted".into())));
    match &app.preview_content {
        Some(s3v::preview::PreviewContent::Text(text)) => {
            assert_eq!(text, "formatted");
        }
        _ => panic!("Expected Text after StreamComplete"),
    }
    assert_eq!(app.preview_scroll, 0);
}

#[test]
fn test_preview_progress() {
    let mut app = app_without_banner();
    app.mode = Mode::Normal;
    let (app, _) = app.handle_event(Event::PreviewProgress {
        received: 1024,
        total: Some(4096),
    });
    match &app.preview_content {
        Some(s3v::preview::PreviewContent::Downloading { received, total }) => {
            assert_eq!(*received, 1024);
            assert_eq!(*total, Some(4096));
        }
        _ => panic!("Expected Downloading"),
    }
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn test_preview_image_ready() {
    let mut app = app_without_banner();
    app.mode = Mode::Normal;
    app.preview_content = Some(s3v::preview::PreviewContent::Downloading {
        received: 4096,
        total: Some(4096),
    });

    let (app, _) = app.handle_event(Event::PreviewImageReady);
    assert!(matches!(
        app.preview_content,
        Some(s3v::preview::PreviewContent::Image)
    ));
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn test_banner_state_transitions() {
    let app = App::new();
    assert_eq!(app.banner_state, s3v::BannerState::Splash);

    // Splash → Active via keypress
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(app.banner_state, s3v::BannerState::Active);

    // Active state persists
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Down)));
    assert_eq!(app.banner_state, s3v::BannerState::Active);
}

#[test]
fn test_utf8_boundary() {
    // 完全な ASCII
    assert_eq!(s3v::preview::text::find_valid_utf8_boundary(b"hello"), 5);

    // 不完全な UTF-8 (日本語の先頭2バイトだけ)
    let incomplete = &[0xe3, 0x81]; // "あ" の先頭2バイト (3バイト文字)
    assert_eq!(s3v::preview::text::find_valid_utf8_boundary(incomplete), 0);

    // 完全な UTF-8 + 不完全な末尾
    let mut mixed_bytes = "あ".as_bytes().to_vec();
    mixed_bytes.extend_from_slice(&[0xe3, 0x81]); // 不完全な末尾
    assert_eq!(
        s3v::preview::text::find_valid_utf8_boundary(&mixed_bytes),
        3
    );
}

#[test]
fn test_search_valid_where_clause() {
    let index = s3v::search::MetadataIndex::new().unwrap();
    let items = vec![S3Item::File {
        name: "test.txt".into(),
        key: "test.txt".into(),
        size: 100,
        last_modified: None,
    }];
    index.insert_items(&items).unwrap();
    let result = index.search("name LIKE '%test%'").unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_auto_preview_on_cursor_move() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.items = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.txt".into(),
            key: "b.txt".into(),
            size: 200,
            last_modified: None,
        },
    ];
    app.mode = Mode::Normal;

    // カーソル下移動 → 自動プレビューコマンドが生成される
    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Down)));
    assert_eq!(app.cursor, 1);
    // RequestPreview が含まれることを確認
    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::RequestPreview { .. })),
        "Expected RequestPreview command on cursor move"
    );
}

#[test]
fn test_parent_items_loaded() {
    let app = app_without_banner();
    let parent_items = vec![
        S3Item::Folder {
            name: "folder-a/".into(),
            prefix: "folder-a/".into(),
        },
        S3Item::Folder {
            name: "folder-b/".into(),
            prefix: "folder-b/".into(),
        },
    ];

    let (app, _) = app.handle_event(Event::ParentItemsLoaded(parent_items));
    assert_eq!(app.parent_items.len(), 2);
}

#[test]
fn test_folder_preview_loaded() {
    let app = app_without_banner();
    let folder_items = vec![S3Item::File {
        name: "child.txt".into(),
        key: "folder/child.txt".into(),
        size: 50,
        last_modified: None,
    }];

    let (app, _) = app.handle_event(Event::FolderPreviewLoaded(folder_items));
    assert_eq!(app.folder_preview_items.len(), 1);
}

#[test]
fn test_prefetch_complete_caches() {
    let app = app_without_banner();

    let (app, _) = app.handle_event(Event::PrefetchComplete {
        key: "test.txt".into(),
        content: "cached content".into(),
    });
    assert_eq!(app.preview_cache.get("test.txt").unwrap(), "cached content");
}

#[test]
fn test_tab_toggles_preview_focus() {
    let mut app = app_without_banner();
    app.mode = Mode::Normal;
    app.preview_content = Some(s3v::preview::PreviewContent::Text("content".into()));

    // Tab → PreviewFocus
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Tab)));
    assert_eq!(app.mode, Mode::PreviewFocus);

    // Tab → back to Normal
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Tab)));
    assert_eq!(app.mode, Mode::Normal);
}

// --- Download Tests ---

#[test]
fn test_download_confirm_single_file() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.items = vec![S3Item::File {
        name: "test.json".into(),
        key: "test.json".into(),
        size: 1024,
        last_modified: None,
    }];
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Char('d'))));
    assert_eq!(app.mode, Mode::DownloadConfirm);
    assert!(app.download_target.is_some());
    assert!(cmds.is_empty());
    assert!(!app.download_path.is_empty());
}

#[test]
fn test_download_confirm_folder() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.items = vec![S3Item::Folder {
        name: "images/".into(),
        prefix: "images/".into(),
    }];
    app.mode = Mode::Normal;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Char('d'))));
    assert_eq!(app.mode, Mode::Loading);
    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::ListFolderFiles { .. }))
    );
}

#[test]
fn test_folder_files_listed() {
    let mut app = app_without_banner();
    app.mode = Mode::Loading;
    app.download_path = "~/Downloads/".into();

    let files = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "images/a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.txt".into(),
            key: "images/b.txt".into(),
            size: 200,
            last_modified: None,
        },
    ];
    let (app, _) = app.handle_event(Event::FolderFilesListed {
        files,
        total_size: 300,
    });
    assert_eq!(app.mode, Mode::DownloadConfirm);
    assert!(matches!(
        app.download_target,
        Some(download::DownloadTarget::Folder { file_count: 2, .. })
    ));
}

#[test]
fn test_download_confirm_cancel_esc() {
    let mut app = app_without_banner();
    app.mode = Mode::DownloadConfirm;
    app.download_target = Some(download::DownloadTarget::SingleFile {
        name: "test.json".into(),
        key: "test.json".into(),
        size: 1024,
    });

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Esc)));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.download_target.is_none());
}

#[test]
fn test_download_confirm_start() {
    let mut app = app_without_banner();
    app.current_path = S3Path::bucket("my-bucket");
    app.mode = Mode::DownloadConfirm;
    app.download_path = "~/Downloads/".into();
    app.download_target = Some(download::DownloadTarget::SingleFile {
        name: "test.json".into(),
        key: "test.json".into(),
        size: 1024,
    });
    app.confirm_focus = download::ConfirmFocus::Buttons;
    app.confirm_button = download::ConfirmButton::Start;

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(app.mode, Mode::Downloading);
    assert!(
        cmds.iter()
            .any(|cmd| matches!(cmd, Command::StartDownload { .. }))
    );
}

#[test]
fn test_download_confirm_button_toggle() {
    let mut app = app_without_banner();
    app.mode = Mode::DownloadConfirm;
    app.confirm_focus = download::ConfirmFocus::Buttons;
    app.confirm_button = download::ConfirmButton::Start;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Right)));
    assert_eq!(app.confirm_button, download::ConfirmButton::Cancel);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Left)));
    assert_eq!(app.confirm_button, download::ConfirmButton::Start);
}

#[test]
fn test_download_confirm_focus_toggle() {
    let mut app = app_without_banner();
    app.mode = Mode::DownloadConfirm;
    app.confirm_focus = download::ConfirmFocus::Buttons;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Up)));
    assert_eq!(app.confirm_focus, download::ConfirmFocus::Path);

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Down)));
    assert_eq!(app.confirm_focus, download::ConfirmFocus::Buttons);
}

#[test]
fn test_download_cancel_button() {
    let mut app = app_without_banner();
    app.mode = Mode::DownloadConfirm;
    app.download_target = Some(download::DownloadTarget::SingleFile {
        name: "test.json".into(),
        key: "test.json".into(),
        size: 1024,
    });
    app.confirm_focus = download::ConfirmFocus::Buttons;
    app.confirm_button = download::ConfirmButton::Cancel;

    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Enter)));
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.download_target.is_none());
}

#[test]
fn test_download_file_complete_progress() {
    let mut app = app_without_banner();
    app.mode = Mode::Downloading;

    let (app, _) = app.handle_event(Event::DownloadFileComplete {
        completed: 3,
        total: 10,
        current_file: "file3.txt".into(),
    });
    assert_eq!(app.mode, Mode::Downloading);
    let progress = app.download_progress.unwrap();
    assert_eq!(progress.completed, 3);
    assert_eq!(progress.total, 10);
    assert_eq!(progress.current_file, "file3.txt");
}

#[test]
fn test_download_all_complete() {
    let mut app = app_without_banner();
    app.mode = Mode::Downloading;
    app.download_target = Some(download::DownloadTarget::SingleFile {
        name: "test.json".into(),
        key: "test.json".into(),
        size: 1024,
    });

    let (app, _) = app.handle_event(Event::DownloadAllComplete { count: 5 });
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.download_target.is_none());
    assert!(app.download_progress.is_none());
    assert_eq!(app.status_message, Some("5 files downloaded".to_string()));
}

#[test]
fn test_download_confirm_q_key_does_not_quit() {
    let mut app = app_without_banner();
    app.mode = Mode::DownloadConfirm;
    app.confirm_focus = download::ConfirmFocus::Path;

    // q キーはパス入力に使われ、Quit にならない
    let (app, _) = app.handle_event(Event::Key(key_event(KeyCode::Char('q'))));
    assert!(app.running);
    assert_eq!(app.mode, Mode::DownloadConfirm);
    assert!(app.download_path.contains('q'));
}

#[test]
fn test_download_multiple_files_selected() {
    let mut app = app_without_banner();
    app.current_path = S3Path::with_prefix("my-bucket", "folder/");
    app.items = vec![
        S3Item::File {
            name: "a.txt".into(),
            key: "folder/a.txt".into(),
            size: 100,
            last_modified: None,
        },
        S3Item::File {
            name: "b.txt".into(),
            key: "folder/b.txt".into(),
            size: 200,
            last_modified: None,
        },
        S3Item::File {
            name: "c.txt".into(),
            key: "folder/c.txt".into(),
            size: 300,
            last_modified: None,
        },
    ];
    app.mode = Mode::Normal;
    // Space で 0, 1 を選択
    app.selected.insert(0);
    app.selected.insert(1);

    let (app, cmds) = app.handle_event(Event::Key(key_event(KeyCode::Char('d'))));
    assert_eq!(app.mode, Mode::DownloadConfirm);
    assert!(cmds.is_empty());

    // MultipleFiles ターゲットで、選択した 2 ファイルが含まれる
    match &app.download_target {
        Some(download::DownloadTarget::MultipleFiles {
            keys,
            total_size,
            base_prefix,
        }) => {
            assert_eq!(keys.len(), 2);
            assert!(keys.contains(&"folder/a.txt".to_string()));
            assert!(keys.contains(&"folder/b.txt".to_string()));
            assert_eq!(*total_size, 300);
            assert_eq!(base_prefix, "folder/");
        }
        other => panic!("Expected MultipleFiles, got {:?}", other),
    }
}
