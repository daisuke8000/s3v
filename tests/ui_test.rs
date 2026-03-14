use ratatui::{Terminal, backend::TestBackend};
use s3v::{App, Event, S3Item, S3Path};

fn render_to_string(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| s3v::ui::render(app, f, None)).unwrap();
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
fn test_ui_renders_startup_banner() {
    let app = App::new(); // banner_state = Splash, mode = Loading
    let output = render_to_string(&app, 80, 24);

    assert!(
        output.contains("s3v") || output.contains("S3 Viewer"),
        "Startup banner should contain 's3v' or 'S3 Viewer'"
    );
    assert!(
        output.contains("Loading"),
        "Startup banner should show 'Loading...'"
    );
}

#[test]
fn test_ui_renders_normal_after_banner_dismissed() {
    let mut app = App::new();
    let items = vec![
        S3Item::Bucket {
            name: "my-bucket-1".to_string(),
        },
        S3Item::Bucket {
            name: "my-bucket-2".to_string(),
        },
    ];
    let (new_app, _) = app.handle_event(Event::ItemsLoaded {
        items,
        next_token: None,
    });
    app = new_app;
    app.banner_state = s3v::BannerState::Active; // キー押下でバナーが閉じた状態をシミュレート

    let output = render_to_string(&app, 80, 24);

    assert!(
        output.contains("my-bucket-1"),
        "Should display first bucket name"
    );
    assert!(
        output.contains("my-bucket-2"),
        "Should display second bucket name"
    );
}

#[test]
fn test_ui_renders_breadcrumb() {
    let mut app = App::new();
    app.banner_state = s3v::BannerState::Active;
    app.current_path = S3Path::with_prefix("test-bucket", "folder/sub/");
    app.items = vec![S3Item::File {
        name: "file.txt".to_string(),
        key: "folder/sub/file.txt".to_string(),
        size: 100,
        last_modified: None,
    }];
    app.mode = s3v::Mode::Normal;

    let output = render_to_string(&app, 80, 24);

    assert!(
        output.contains("test-bucket"),
        "Breadcrumb should contain bucket name"
    );
    assert!(
        output.contains("folder"),
        "Breadcrumb should contain folder path"
    );
}

#[test]
fn test_ui_renders_rounded_borders() {
    let mut app = App::new();
    app.banner_state = s3v::BannerState::Active;
    app.items = vec![S3Item::Bucket {
        name: "bucket".to_string(),
    }];
    app.mode = s3v::Mode::Normal;

    let output = render_to_string(&app, 80, 24);

    assert!(
        output.contains('╭')
            && output.contains('╮')
            && output.contains('╰')
            && output.contains('╯'),
        "Should render rounded borders"
    );
}

#[test]
fn test_ui_renders_help_bar() {
    let mut app = App::new();
    app.banner_state = s3v::BannerState::Active;
    app.mode = s3v::Mode::Normal;

    let output = render_to_string(&app, 120, 24);

    assert!(output.contains("Move"), "Help should mention Move");
    assert!(output.contains("Open"), "Help should mention Open");
    assert!(output.contains("Quit"), "Help should mention Quit");
    assert!(output.contains("Filter"), "Help should mention Filter");
    assert!(output.contains("SQL"), "Help should mention SQL");
}

#[test]
fn test_ui_renders_url_bar() {
    let mut app = App::new();
    app.banner_state = s3v::BannerState::Active;
    app.current_path = S3Path::bucket("test-bucket");
    app.items = vec![S3Item::File {
        name: "test.txt".to_string(),
        key: "test.txt".to_string(),
        size: 100,
        last_modified: None,
    }];
    app.mode = s3v::Mode::Normal;

    let output = render_to_string(&app, 80, 24);

    assert!(
        output.contains("s3://test-bucket/test.txt"),
        "URL bar should show S3 URI of selected item"
    );
}
