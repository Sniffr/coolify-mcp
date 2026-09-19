use safety::frame_untrusted;
use serde::Serialize;
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DocHit {
    pub title: String,
    pub text: String,
    pub score: usize,
}
#[derive(Clone, Debug)]
pub struct DocsSearchEngine {
    docs: Vec<(&'static str, &'static str)>,
}
impl DocsSearchEngine {
    pub fn embedded() -> Self {
        Self {
            docs: vec![
                (
                    "Applications",
                    "Deploy and troubleshoot Coolify applications.",
                ),
                (
                    "Deployments",
                    "Deployment logs describe build and runtime failures.",
                ),
                (
                    "Servers",
                    "Servers provide infrastructure health and resources.",
                ),
            ],
        }
    }
    pub fn search(&self, query: &str, limit: usize) -> Vec<DocHit> {
        let terms: Vec<_> = query
            .split_whitespace()
            .map(|x| x.to_ascii_lowercase())
            .collect();
        let mut hits: Vec<_> = self
            .docs
            .iter()
            .filter_map(|(title, text)| {
                let hay = format!("{} {}", title, text).to_ascii_lowercase();
                let score = terms
                    .iter()
                    .filter(|term| hay.contains(term.as_str()))
                    .count();
                (score > 0).then(|| DocHit {
                    title: (*title).into(),
                    text: frame_untrusted(text, "docs"),
                    score,
                })
            })
            .collect();
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
        hits.truncate(limit.min(50));
        hits
    }
}
