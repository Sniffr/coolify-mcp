use crate::ToolSpec;
#[derive(Clone, Debug, PartialEq)]
pub struct Prompt {
    pub name: String,
    pub description: String,
    pub required_tools: Vec<String>,
}
pub type PromptRegistry = Vec<Prompt>;
pub fn register_prompts(tools: &[ToolSpec]) -> PromptRegistry {
    let names: std::collections::HashSet<_> = tools.iter().map(|t| t.name.as_str()).collect();
    [
        (
            "troubleshoot_application",
            "Investigate an application",
            vec![
                "get_application",
                "diagnose_app",
                "application_logs",
                "application",
            ],
        ),
        (
            "explain_failed_deploy",
            "Explain a failed deployment",
            vec!["get_application", "deployment", "logs", "deploy"],
        ),
        (
            "estate_health",
            "Summarize estate health",
            vec!["get_infrastructure_overview", "list_servers", "find_issues"],
        ),
    ]
    .into_iter()
    .filter(|(_, _, required)| required.iter().all(|x| names.contains(x)))
    .map(|(name, description, required_tools)| Prompt {
        name: name.into(),
        description: description.into(),
        required_tools: required_tools.into_iter().map(str::to_owned).collect(),
    })
    .collect()
}
