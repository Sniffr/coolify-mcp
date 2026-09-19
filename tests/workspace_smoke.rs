#[test]
fn workspace_binary_has_a_main_entrypoint() {
    assert!(std::path::Path::new("src/main.rs").exists());
}
