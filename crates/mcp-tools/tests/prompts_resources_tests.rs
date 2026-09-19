use mcp_tools::docs_search::DocsSearchEngine;
use mcp_tools::prompts::register_prompts;
use mcp_tools::registered_tools;
use mcp_tools::resources::register_resources;
use safety::CapabilityProfile;

#[test]
fn readonly_prompts_only_advertise_available_tools() {
    let tools = registered_tools(CapabilityProfile::ReadOnly, None);
    let prompts = register_prompts(&tools);
    assert!(prompts.iter().any(|p| p.name == "estate_health"));
    assert!(!prompts.iter().any(|p| p.name == "troubleshoot_application"));
    assert!(!prompts.iter().any(|p| p.name == "explain_failed_deploy"));
}

#[test]
fn resources_are_registered_with_read_only_metadata() {
    let resources = register_resources();
    assert!(resources.get("coolify://overview").is_some());
    assert!(resources.get("coolify://application/{uuid}").is_some());
    assert!(resources.get("coolify://overview").unwrap().read_only);
}

#[test]
fn search_is_bounded_deterministic_and_framed_as_untrusted() {
    let engine = DocsSearchEngine::embedded();
    let a = engine.search("application deployment", 2);
    let b = engine.search("application deployment", 2);
    assert_eq!(
        a.iter().map(|x| (&x.title, x.score)).collect::<Vec<_>>(),
        b.iter().map(|x| (&x.title, x.score)).collect::<Vec<_>>()
    );
    assert!(a.len() <= 2);
    assert!(
        a.iter()
            .all(|hit| hit.text.contains("BEGIN UNTRUSTED LOG OUTPUT"))
    );
}
