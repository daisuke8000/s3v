use ratatui::{Terminal, backend::TestBackend};
use s3v::{App, Event, S3Item, S3Path};

fn render_to_string(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| s3v::ui::render(app, f)).unwrap();
    let buffer = terminal.backend().buffer().clone();

    let mut output = String::new();
    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }
    output
}

#[test]
fn test_ui_renders_header_with_root_path() {
    let app = App::new();
    let output = render_to_string(&app, 80, 20);

    // ヘッダーに "s3v" と "/" が表示される
    assert!(output.contains("s3v"), "Header should contain 's3v'");
    assert!(output.contains("/"), "Header should show root path '/'");
}

#[test]
fn test_ui_renders_loading_state() {
    let app = App::new(); // mode = Loading by default
    let output = render_to_string(&app, 80, 20);

    assert!(
        output.contains("Loading"),
        "Should show 'Loading...' in loading state"
    );
}

#[test]
fn test_ui_renders_bucket_list() {
    let app = App::new();
    let items = vec![
        S3Item::Bucket {
            name: "my-bucket-1".to_string(),
        },
        S3Item::Bucket {
            name: "my-bucket-2".to_string(),
        },
    ];
    let (app, _) = app.handle_event(Event::ItemsLoaded(items));

    let output = render_to_string(&app, 80, 20);

    assert!(
        output.contains("my-bucket-1"),
        "Should display first bucket name"
    );
    assert!(
        output.contains("my-bucket-2"),
        "Should display second bucket name"
    );
    assert!(output.contains("2 items"), "Should show item count");
}

#[test]
fn test_ui_renders_file_list_with_size() {
    let mut app = App::new();
    app.current_path = S3Path::bucket("test-bucket");
    let items = vec![
        S3Item::Folder {
            name: "docs/".to_string(),
            prefix: "docs/".to_string(),
        },
        S3Item::File {
            name: "readme.txt".to_string(),
            key: "readme.txt".to_string(),
            size: 2048,
            last_modified: Some("2024-03-15T10:30:00Z".to_string()),
        },
    ];
    let (app, _) = app.handle_event(Event::ItemsLoaded(items));

    let output = render_to_string(&app, 80, 20);

    assert!(output.contains("docs/"), "Should display folder name");
    assert!(output.contains("readme.txt"), "Should display file name");
    assert!(output.contains("2.0 KB"), "Should format file size");
    assert!(output.contains("2024-03-15"), "Should display date");
    assert!(output.contains("DIR"), "Folder should have DIR icon");
}

#[test]
fn test_ui_renders_help_bar() {
    let app = App::new();
    let output = render_to_string(&app, 80, 20);

    assert!(output.contains("Enter"), "Help should mention Enter");
    assert!(output.contains("Esc"), "Help should mention Esc");
    assert!(output.contains("Quit"), "Help should mention Quit");
}

#[test]
fn test_ui_renders_url_bar() {
    let mut app = App::new();
    app.current_path = S3Path::bucket("test-bucket");
    let items = vec![S3Item::File {
        name: "test.txt".to_string(),
        key: "test.txt".to_string(),
        size: 100,
        last_modified: None,
    }];
    let (app, _) = app.handle_event(Event::ItemsLoaded(items));

    let output = render_to_string(&app, 80, 20);

    assert!(
        output.contains("s3://test-bucket/test.txt"),
        "URL bar should show S3 URI of selected item"
    );
}
