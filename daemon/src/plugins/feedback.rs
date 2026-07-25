use std::{collections::HashMap, sync::RwLock};

use crate::models::{feedback::Feedback, identifiers::IntegrationId};

/// Identifies one evaluated feedback. Two buttons watching the same light share a key, and so
/// share a single cache entry and a single evaluation.
///
/// `parameters` is the serialized JSON rather than the `Value` itself because `serde_json::Value`
/// is neither `Hash` nor `Eq`. Its object maps are sorted, so the string is canonical.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct FeedbackKey {
    pub integration_id: IntegrationId,
    pub feedback_name: String,
    parameters: String,
}

impl FeedbackKey {
    pub fn new(feedback: &Feedback) -> Self {
        Self {
            integration_id: feedback.integration_id.clone(),
            feedback_name: feedback.feedback_name.clone(),
            parameters: serde_json::to_string(&feedback.parameters)
                .unwrap_or_else(|_| feedback.parameters.to_string()),
        }
    }

    pub fn parameters(&self) -> serde_json::Value {
        serde_json::from_str(&self.parameters).unwrap_or(serde_json::Value::Null)
    }
}

#[derive(Default)]
pub struct FeedbackCache {
    results: RwLock<HashMap<FeedbackKey, bool>>,
}

impl FeedbackCache {
    /// Absent means "not evaluated yet", which renders as inactive. A feedback whose plugin has
    /// not started should leave the button looking like its configured state rather than blank.
    pub fn get(&self, key: &FeedbackKey) -> Option<bool> {
        self.results.read().unwrap().get(key).copied()
    }

    pub fn is_active(&self, key: &FeedbackKey) -> bool {
        self.get(key).unwrap_or(false)
    }

    pub fn set(&self, key: FeedbackKey, result: bool) -> bool {
        let mut results = self.results.write().unwrap();
        match results.insert(key, result) {
            Some(previous) => previous != result,
            None => true,
        }
    }

    pub fn clear_instance(&self, integration_id: &IntegrationId) -> Vec<FeedbackKey> {
        let mut results = self.results.write().unwrap();
        let cleared: Vec<_> = results
            .keys()
            .filter(|key| key.integration_id == *integration_id)
            .cloned()
            .collect();
        for key in &cleared {
            results.remove(key);
        }
        cleared
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feedback(parameters: serde_json::Value) -> Feedback {
        Feedback {
            integration_id: IntegrationId("hass.home".to_string()),
            feedback_name: "state_is".to_string(),
            parameters,
        }
    }

    #[test]
    fn parameter_order_does_not_change_the_key() {
        let left = FeedbackKey::new(&feedback(
            serde_json::json!({ "entity_id": "light.kitchen", "state": "on" }),
        ));
        let right = FeedbackKey::new(&feedback(
            serde_json::json!({ "state": "on", "entity_id": "light.kitchen" }),
        ));
        assert_eq!(left, right);
    }

    #[test]
    fn different_parameters_are_different_keys() {
        let kitchen = FeedbackKey::new(&feedback(
            serde_json::json!({ "entity_id": "light.kitchen" }),
        ));
        let hallway = FeedbackKey::new(&feedback(
            serde_json::json!({ "entity_id": "light.hallway" }),
        ));
        assert_ne!(kitchen, hallway);
    }

    #[test]
    fn an_unevaluated_feedback_is_inactive_rather_than_unknown() {
        let cache = FeedbackCache::default();
        let key = FeedbackKey::new(&feedback(serde_json::json!({})));
        assert_eq!(cache.get(&key), None);
        assert!(!cache.is_active(&key));
    }

    #[test]
    fn setting_an_unchanged_result_reports_no_change() {
        let cache = FeedbackCache::default();
        let key = FeedbackKey::new(&feedback(serde_json::json!({})));
        assert!(cache.set(key.clone(), true));
        assert!(!cache.set(key.clone(), true));
        assert!(cache.set(key, false));
    }
}
