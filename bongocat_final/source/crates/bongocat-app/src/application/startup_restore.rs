//! Restoring the configured model at startup, and the fallback that keeps a
//! selection which no longer loads from becoming a startup that never presents a
//! model at all.
//!
//! A model that is slow is not a model that is gone. The first startup attempt
//! runs under a short budget and a transient expiry is retried under growing
//! budgets before the selection is treated as unusable: a machine that has just
//! woken up can spend several seconds reading the Cubism model and its textures,
//! and the configured model must not be replaced because of that. Only a
//! selection that stays unusable after every budget falls back to the standard
//! preset, and only a standard preset that is unusable as well ends the startup.
//! That last failure is recorded as an anonymous `application/startup_failed`
//! event, because the process exits before any window exists: a login item's
//! stderr goes nowhere, and a launch that shows no window would otherwise leave
//! no reason behind.

use super::Application;
use crate::ApplicationError;
use crate::app_log::{ApplicationLogCode, ApplicationLogContext, ApplicationLogEvent};
use crate::model_identity::{config_source_from_model, model_origin_from_config};
use crate::model_listing::STANDARD_PRESET_MODEL_ID;
use bongocat_config::ModelIdentity;
use bongocat_model::{ModelId, ModelOrigin};
use std::time::Duration;

/// The wait budgets a startup activation tries in order.
///
/// The first one keeps the common case fast: the runtime reports a prepared
/// model long before it expires. The later ones exist for a cold load, which is
/// the case that used to end the process — after a long sleep the GPU and the
/// disk are both busy, and the interactive two-second wait is not enough for the
/// model to reach the renderer.
const STARTUP_PREPARE_TIMEOUTS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(10),
    Duration::from_secs(15),
];

/// Pause between two startup activation attempts, on the main thread before any
/// window exists.
const STARTUP_PREPARE_BACKOFF: Duration = Duration::from_secs(1);

impl Application {
    /// Restore the model selection at startup. The configured selection is
    /// activated when it still loads; a selection whose resources were
    /// deleted by hand or are otherwise unusable never blocks startup — the
    /// application records an anonymous fallback event, persists the
    /// always-available standard preset as the corrected selection, and
    /// activates it. Without a configured selection the standard preset is
    /// the default model. Metadata records for model directories that no
    /// longer exist are pruned while configuration is operational.
    ///
    /// A transient activation expiry is retried before the selection counts as
    /// unusable, so a cold model load no longer replaces the user's choice. The
    /// runtime keeps at most one unresolved model activation: a pending
    /// activation is only committed once the overlay frame source consumes its
    /// first prepared frame, and a retry replaces the pending one.
    pub fn restore_startup_model(&mut self) -> Result<(), ApplicationError> {
        self.prune_missing_installed_metadata();
        let configured = self.config.model.selected_model.as_ref().map(|selected| {
            (
                selected.id.clone(),
                model_origin_from_config(selected.source),
            )
        });
        let restored = match configured {
            // No configured selection: the standard preset is the default model.
            None => self.prepare_startup_model(ModelOrigin::Preset, self.standard_preset_id()),
            Some((id, origin)) => match ModelId::parse(id) {
                Ok(id) => match self.prepare_startup_model(origin, id) {
                    Ok(()) => Ok(()),
                    // Every budget expired or the model was rejected: the
                    // selection is unusable, so the standard preset takes over
                    // exactly as the fallback always did.
                    Err(error) => {
                        self.fallback_to_standard_preset(error.stable_code());
                        self.prepare_startup_model(ModelOrigin::Preset, self.standard_preset_id())
                    }
                },
                Err(_) => {
                    self.fallback_to_standard_preset("model_id_invalid");
                    self.prepare_startup_model(ModelOrigin::Preset, self.standard_preset_id())
                }
            },
        };
        match restored {
            Ok(()) => Ok(()),
            Err(error) => {
                self.application_log.record(
                    ApplicationLogEvent::new(ApplicationLogCode::StartupFailed)
                        .with_context(ApplicationLogContext::Phase("model_restore"))
                        .with_context(ApplicationLogContext::Reason(error.stable_code())),
                );
                Err(error)
            }
        }
    }

    /// Activate one model with the startup budgets, retrying only a transient
    /// expiry.
    fn prepare_startup_model(
        &mut self,
        origin: ModelOrigin,
        id: ModelId,
    ) -> Result<(), ApplicationError> {
        retry_transient(
            &STARTUP_PREPARE_TIMEOUTS,
            |budget| {
                self.prepare_model_with_timeout(origin, id.as_str(), budget)
                    .map(|_| ())
            },
            startup_prepare_is_transient,
            std::thread::sleep,
        )
    }

    /// Record the anonymous fallback event and persist the standard preset
    /// as the corrected selection. A failed commit keeps the stale selection
    /// on disk; the next startup simply retries the fallback.
    fn fallback_to_standard_preset(&mut self, reason: &'static str) {
        self.application_log.record(
            ApplicationLogEvent::model_selection_fallback()
                .with_context(ApplicationLogContext::Reason(reason)),
        );
        self.persist_model_selection(ModelOrigin::Preset, &self.standard_preset_id());
    }

    fn standard_preset_id(&self) -> ModelId {
        ModelId::parse(STANDARD_PRESET_MODEL_ID).expect("standard preset model id is valid")
    }

    /// Persist a corrected model selection without touching the runtime.
    /// A failed commit keeps the stale selection on disk; the next startup
    /// simply retries the fallback.
    fn persist_model_selection(&mut self, origin: ModelOrigin, id: &ModelId) {
        let mut next_config = self.config.clone();
        next_config.model.selected_model = Some(ModelIdentity {
            id: id.as_str().to_owned(),
            source: config_source_from_model(origin),
        });
        let Ok(expected_revision) = self.ready_config_revision() else {
            return;
        };
        let Ok(next_revision) = self
            .config_store
            .commit_if_revision(&next_config, expected_revision)
        else {
            return;
        };
        self.config = next_config;
        self.config_revision = Some(next_revision);
    }
}

/// Whether a failed startup activation can succeed on another attempt.
///
/// Only "the runtime did not publish a prepared model within the budget" is
/// retried: it is the one failure a cold or busy machine produces, and it says
/// nothing about the model. A model that is missing, unparsable or rejected by
/// the runtime stays a deterministic failure, so a genuinely broken selection is
/// not retried for half a minute first.
fn startup_prepare_is_transient(error: &ApplicationError) -> bool {
    matches!(error, ApplicationError::RuntimeDidNotPrepareModel)
}

/// Run `attempt` once per budget, in order, retrying only failures `is_transient`
/// accepts and pausing between attempts.
///
/// `budgets` must not be empty: the last budget always returns, because there is
/// nothing left to wait for after it.
fn retry_transient<E>(
    budgets: &[Duration],
    mut attempt: impl FnMut(Duration) -> Result<(), E>,
    is_transient: impl Fn(&E) -> bool,
    mut pause: impl FnMut(Duration),
) -> Result<(), E> {
    let mut last_error = None;
    for (index, budget) in budgets.iter().enumerate() {
        match attempt(*budget) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retryable = is_transient(&error) && index + 1 < budgets.len();
                last_error = Some(error);
                if !retryable {
                    break;
                }
            }
        }
        pause(STARTUP_PREPARE_BACKOFF);
    }
    Err(last_error.expect("a retry attempts at least one budget"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RUNTIME_TIMEOUT;

    #[derive(Debug, Eq, PartialEq)]
    enum PrepareError {
        Transient,
        Fatal,
    }

    fn budgets(count: u64) -> Vec<Duration> {
        (1..=count).map(Duration::from_millis).collect()
    }

    #[test]
    fn a_transient_expiry_is_retried_under_the_next_budget() {
        let mut attempts = Vec::new();
        let mut pauses = 0;
        let result = retry_transient(
            &budgets(3),
            |budget| {
                attempts.push(budget);
                if attempts.len() < 3 {
                    Err(PrepareError::Transient)
                } else {
                    Ok(())
                }
            },
            |error| error == &PrepareError::Transient,
            |_| pauses += 1,
        );
        assert_eq!(result, Ok(()));
        assert_eq!(attempts, budgets(3));
        assert_eq!(pauses, 2);
    }

    #[test]
    fn a_deterministic_failure_is_not_retried() {
        let mut calls = 0;
        let result = retry_transient(
            &budgets(3),
            |_| {
                calls += 1;
                Err::<(), _>(PrepareError::Fatal)
            },
            |error| error == &PrepareError::Transient,
            |_| panic!("a deterministic failure must not pause"),
        );
        assert_eq!(result, Err(PrepareError::Fatal));
        assert_eq!(calls, 1);
    }

    #[test]
    fn exhausted_budgets_report_the_last_transient_failure() {
        let mut calls = 0;
        let result = retry_transient(
            &budgets(3),
            |_| {
                calls += 1;
                Err::<(), _>(PrepareError::Transient)
            },
            |error| error == &PrepareError::Transient,
            |_| {},
        );
        assert_eq!(result, Err(PrepareError::Transient));
        assert_eq!(calls, 3);
    }

    #[test]
    fn only_a_missing_prepared_model_is_transient() {
        assert!(startup_prepare_is_transient(
            &ApplicationError::RuntimeDidNotPrepareModel
        ));
        assert!(!startup_prepare_is_transient(
            &ApplicationError::ModelNotFound(
                ModelId::parse(STANDARD_PRESET_MODEL_ID).expect("standard preset id")
            )
        ));
    }

    #[test]
    fn startup_budgets_leave_room_for_a_cold_model_load() {
        assert_eq!(STARTUP_PREPARE_TIMEOUTS.len(), 3);
        assert!(
            STARTUP_PREPARE_TIMEOUTS
                .iter()
                .all(|budget| *budget > RUNTIME_TIMEOUT),
            "a startup budget must exceed the interactive wait it was split from"
        );
        assert!(
            STARTUP_PREPARE_TIMEOUTS
                .windows(2)
                .all(|pair| pair[1] >= pair[0]),
            "the budgets grow, so a cold load gets the longest wait"
        );
    }
}
