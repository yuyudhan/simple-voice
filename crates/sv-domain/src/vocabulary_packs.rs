// FilePath: crates/sv-domain/src/vocabulary_packs.rs
//! Built-in vocabulary packs: curated spellings for the kinds of work people dictate about, so
//! recognition writes "Kubernetes" or "HubSpot" without the user typing them into the
//! dictionary first. The ids and names live here because settings store them and the UI lists
//! them; the words themselves live in sv-text.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyPack {
    Everyday,
    Programming,
    Web,
    Backend,
    Mobile,
    Cloud,
    DataAi,
    Sales,
    Marketing,
    Business,
}

impl VocabularyPack {
    /// Every pack, in the order the Dictionary page lists them.
    pub const ALL: [VocabularyPack; 10] = [
        Self::Everyday,
        Self::Programming,
        Self::Web,
        Self::Backend,
        Self::Mobile,
        Self::Cloud,
        Self::DataAi,
        Self::Sales,
        Self::Marketing,
        Self::Business,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Everyday => "Everyday tech",
            Self::Programming => "Programming",
            Self::Web => "Web & JavaScript",
            Self::Backend => "Backend & databases",
            Self::Mobile => "Mobile apps",
            Self::Cloud => "Cloud & DevOps",
            Self::DataAi => "Data & AI",
            Self::Sales => "Sales",
            Self::Marketing => "Marketing",
            Self::Business => "Business & finance",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Everyday => "Devices, apps and AI assistants everyone mentions",
            Self::Programming => "Programming languages, Git, editors and core terms",
            Self::Web => "Frameworks, build tools and libraries of the web",
            Self::Backend => "Server frameworks, databases, queues and APIs",
            Self::Mobile => "iOS and Android development",
            Self::Cloud => "Cloud providers, containers, CI/CD and observability",
            Self::DataAi => "Machine learning, LLMs, notebooks and data tools",
            Self::Sales => "CRMs, sales tools, deal stages and SaaS metrics",
            Self::Marketing => "Ad platforms, analytics, SEO and campaign metrics",
            Self::Business => "Finance, management, legal and back-office tools",
        }
    }
}

/// A new install recognises everyday tech and programming terms; the rest are opt-in, because
/// each enabled pack shares the recognition prompt with the others.
pub fn default_vocabulary_packs() -> Vec<VocabularyPack> {
    vec![VocabularyPack::Everyday, VocabularyPack::Programming]
}

/// One pack as the Dictionary page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VocabularyPackInfo {
    pub id: VocabularyPack,
    pub name: String,
    pub description: String,
    /// Every spelling the pack teaches, as written, in its priority order.
    pub terms: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_use_the_wire_names_the_ui_sends() {
        let parsed: Result<Vec<VocabularyPack>, _> =
            serde_json::from_str(r#"["everyday","data_ai","business"]"#);
        assert_eq!(
            parsed.ok(),
            Some(vec![
                VocabularyPack::Everyday,
                VocabularyPack::DataAi,
                VocabularyPack::Business
            ])
        );
    }
}
