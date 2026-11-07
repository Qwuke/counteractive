//! The release catalogue as the gate serves it: each model's terms and the
//! backend that runs it (docs 05 section 1).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context as _, bail};
use backend::{InferenceBackend, OpenAiBackend};
use manifest::CatalogueEntry;
use metering::ModelTerms;

/// Engines this build serves. All three speak the OpenAI-compatible API
/// with vLLM's `/tokenize`, so one adapter covers them.
const OPENAI_COMPATIBLE_ENGINES: [&str; 3] = ["vllm", "llamacpp", "fake"];

/// One servable model.
pub struct Model {
    /// The id clients ask for, also sent to the engine.
    pub id: String,
    /// Its prices and output cap.
    pub terms: ModelTerms,
    /// What runs it.
    pub backend: Arc<dyn InferenceBackend>,
}

/// Every model in the catalogue, by id.
pub struct Models(HashMap<String, Model>);

impl Models {
    /// Serves each catalogue entry from the engine on `engine`.
    ///
    /// Fails if an entry names an engine this build can't drive, or a
    /// `usage_trust` its backend doesn't provide: a model is refused at
    /// startup, never quietly served on weaker terms.
    pub fn from_catalogue(catalogue: &[CatalogueEntry], engine: &Path) -> anyhow::Result<Self> {
        let backend: Arc<dyn InferenceBackend> = Arc::new(OpenAiBackend::new(engine));
        let trust = backend.describe().usage_trust.as_str();
        let mut models = HashMap::new();
        for entry in catalogue {
            if !OPENAI_COMPATIBLE_ENGINES.contains(&entry.engine.as_str()) {
                bail!("model {}: engine {:?} is not one this gate can serve", entry.id, entry.engine);
            }
            if entry.usage_trust != trust {
                bail!(
                    "model {}: catalogue says usage_trust {:?} but its backend gives {trust:?}",
                    entry.id,
                    entry.usage_trust
                );
            }
            let terms = ModelTerms::new(entry.input_rate, entry.output_rate, entry.max_output)
                .with_context(|| format!("model {}", entry.id))?;
            models.insert(entry.id.clone(), Model { id: entry.id.clone(), terms, backend: Arc::clone(&backend) });
        }
        Ok(Self(models))
    }

    /// The model with this id.
    pub fn get(&self, id: &str) -> Option<&Model> {
        self.0.get(id)
    }
}
