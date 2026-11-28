//! Common workflow result envelope: completion vs coverage vs evidence.
//! Success text stays brief and never duplicates structured data.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Complete,
    Partial,
    AmbiguousEntity,
    NotFound,
    Unsupported,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionStatus {
    Ok,
    Unsupported,
    Disabled,
    Unavailable,
    Unknown,
    NotFound,
    Timeout,
    BudgetExhausted,
}

#[derive(Clone, Debug)]
pub struct SectionResult {
    pub name: &'static str,
    pub status: SectionStatus,
    pub data: Option<Value>,
    pub error: Option<String>,
    pub at: String,
}

#[derive(Clone, Debug, Default)]
pub struct Envelope {
    pub completion: Option<Completion>,
    pub coverage: String,
    pub query: Value,
    pub evidence: Vec<Value>,
    pub warnings: Vec<String>,
    pub limitations: Vec<String>,
    pub sources: Vec<String>,
    pub next_actions: Vec<String>,
    pub sections: Vec<SectionResult>,
}

impl Envelope {
    pub fn brief_text(&self) -> String {
        match self.completion {
            Some(Completion::Complete) => "Workflow complete.".to_owned(),
            Some(Completion::Partial) => "Workflow partial; see sections.".to_owned(),
            Some(Completion::AmbiguousEntity) => "Ambiguous entity; candidates listed.".to_owned(),
            Some(Completion::NotFound) => "Entity not found.".to_owned(),
            Some(Completion::Unsupported) => "Operation unsupported.".to_owned(),
            Some(Completion::Unavailable) => "Upstream unavailable.".to_owned(),
            None => "Workflow result.".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Completion, Envelope};

    #[test]
    fn completion_and_coverage_stay_separate() {
        let envelope = Envelope {
            completion: Some(Completion::Partial),
            coverage: "2 of 3 sections".to_owned(),
            ..Default::default()
        };
        assert_eq!(envelope.brief_text(), "Workflow partial; see sections.");
        assert!(envelope.brief_text().len() < envelope.coverage.len() + 40);
    }
}
