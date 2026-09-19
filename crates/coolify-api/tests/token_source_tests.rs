use std::collections::HashMap;
use std::fs;

use coolify_api::TokenSource;

#[test]
fn inline_token_has_redacted_debug_and_refreshes() {
    let source = TokenSource::from_env(&HashMap::from([(
        "COOLIFY_ACCESS_TOKEN".into(),
        "token-a".into(),
    )]))
    .unwrap();
    assert_eq!(source.current().unwrap(), "token-a");
    assert!(!format!("{source:?}").contains("token-a"));
    source.refresh().unwrap();
    assert_eq!(source.current().unwrap(), "token-a");
}

#[test]
fn file_token_trims_trailing_whitespace_and_wins() {
    let path = std::env::temp_dir().join(format!("coolify-token-{}", std::process::id()));
    fs::write(&path, "file-secret\n\n").unwrap();
    let source = TokenSource::from_file(&path).unwrap();
    assert_eq!(source.current().unwrap(), "file-secret");
    assert!(!format!("{source:?}").contains("file-secret"));
    fs::remove_file(path).unwrap();
}
