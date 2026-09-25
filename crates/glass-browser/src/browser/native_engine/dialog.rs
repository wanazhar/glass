use super::config::{validate_context_id, validate_url_text};
use super::error::NativeEngineError;
use super::javascript::{MAX_NATIVE_DIALOG_TEXT_BYTES, NativeDialog};
use crate::browser::session::PendingDialog;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

/// Maximum UTF-8 byte length accepted for native JavaScript dialog text.
pub const NATIVE_DIALOG_TEXT_LIMIT_BYTES: usize = MAX_NATIVE_DIALOG_TEXT_BYTES;

/// A process-backed modal JavaScript dialog awaiting a host decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativePendingDialog {
    /// Stable identity for resolving this exact pending dialog.
    pub id: String,
    /// Owning browsing context (the target identity for this native session).
    pub context_id: String,
    /// Frame whose script is suspended at the dialog call.
    pub frame_id: String,
    /// Bounded dialog kind, message, default prompt value, and source URL.
    pub dialog: PendingDialog,
}

/// The selected host response for a native JavaScript dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeDialogResolution {
    /// Accept the dialog; `false` dismisses it.
    pub accepted: bool,
    /// Response text for an accepted `prompt`, if overriding its default.
    #[serde(default)]
    pub prompt_value: Option<String>,
}

pub(crate) struct NativeDialogWait {
    pub(crate) resolution: NativeDialogResolution,
    cleanup: PendingDialogCleanup,
}

impl NativeDialogWait {
    pub(crate) fn finish(self) -> NativeDialogResolution {
        self.cleanup.control.clear_if_pending(&self.cleanup.id);
        self.resolution
    }
}

struct ActiveDialog {
    pending: NativePendingDialog,
    child_dialog_id: u64,
    response: Option<oneshot::Sender<NativeDialogResolution>>,
}

#[derive(Default)]
struct DialogState {
    active: Option<ActiveDialog>,
}

struct NativeDialogControlInner {
    state: Mutex<DialogState>,
    next_id: AtomicU64,
    modal_dialogs: bool,
}

/// Out-of-band rendezvous shared by the active browser operation and its
/// control plane. It deliberately does not require the page-state mutex, which
/// is held while a content-process script is suspended in a modal dialog.
#[derive(Clone)]
pub(crate) struct NativeDialogControlPlane(Arc<NativeDialogControlInner>);

/// Out-of-band control for modal dialogs in an explicitly configured native
/// session. Clones refer to the same pending-dialog state and can be used
/// while a navigation or evaluation future is suspended.
#[derive(Clone)]
pub struct NativeDialogController {
    control: NativeDialogControlPlane,
}

impl NativeDialogController {
    pub(crate) fn new(control: NativeDialogControlPlane) -> Result<Self, NativeEngineError> {
        if !control.modal_dialogs_enabled() {
            return Err(NativeEngineError::invalid(
                "native dialog controller",
                "modal dialogs were not enabled for this session",
            ));
        }
        Ok(Self { control })
    }

    /// Return the active process-backed modal dialog without waiting for the
    /// session's serialized page-operation lock.
    pub fn pending_dialog(&self) -> Result<Option<NativePendingDialog>, NativeEngineError> {
        self.control.pending()
    }

    /// Resolve exactly `dialog_id`. A stale, duplicate, or mismatched ID is
    /// rejected and leaves the active dialog unchanged.
    pub fn resolve_dialog(
        &self,
        dialog_id: &str,
        resolution: NativeDialogResolution,
    ) -> Result<(), NativeEngineError> {
        self.control
            .resolve(dialog_id, resolution.accepted, resolution.prompt_value)
            .map(|_| ())
    }
}

impl Default for NativeDialogControlPlane {
    fn default() -> Self {
        Self(Arc::new(NativeDialogControlInner {
            state: Mutex::new(DialogState::default()),
            next_id: AtomicU64::new(1),
            modal_dialogs: false,
        }))
    }
}

impl NativeDialogControlPlane {
    pub(crate) fn for_modal_owner() -> Self {
        Self(Arc::new(NativeDialogControlInner {
            state: Mutex::new(DialogState::default()),
            next_id: AtomicU64::new(1),
            modal_dialogs: true,
        }))
    }

    pub(crate) fn modal_dialogs_enabled(&self) -> bool {
        self.0.modal_dialogs
    }

    pub(crate) fn pending(&self) -> Result<Option<NativePendingDialog>, NativeEngineError> {
        self.0
            .state
            .lock()
            .map(|state| {
                state
                    .active
                    .as_ref()
                    .filter(|active| active.response.is_some())
                    .map(|active| active.pending.clone())
            })
            .map_err(|_| NativeEngineError::Worker {
                operation: "inspect native dialog".into(),
                reason: "native dialog control state is unavailable".into(),
            })
    }

    pub(crate) async fn wait_for_resolution(
        &self,
        context_id: &str,
        frame_id: &str,
        url: &str,
        child_dialog_id: u64,
        dialog: NativeDialog,
    ) -> Result<NativeDialogWait, NativeEngineError> {
        validate_dialog(&dialog)?;
        validate_context_id(context_id)?;
        validate_context_id(frame_id)?;
        validate_url_text("native dialog URL", url)?;
        if child_dialog_id == 0 {
            return Err(NativeEngineError::invalid(
                "native dialog ID",
                "child identifier must be positive",
            ));
        }
        let id = self
            .0
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| {
                NativeEngineError::invalid("native dialog ID", "identifier space exhausted")
            })?;
        let pending = NativePendingDialog {
            id: format!("native-dialog-{id}"),
            context_id: context_id.to_owned(),
            frame_id: frame_id.to_owned(),
            dialog: PendingDialog {
                dialog_type: dialog.dialog_type,
                message: dialog.message,
                default_value: dialog.default_value,
                url: url.to_owned(),
            },
        };
        let (response, receiver) = oneshot::channel();
        {
            let mut state = self.0.state.lock().map_err(|_| NativeEngineError::Worker {
                operation: "open native dialog".into(),
                reason: "native dialog control state is unavailable".into(),
            })?;
            if state.active.is_some() {
                return Err(NativeEngineError::invalid(
                    "native dialog state",
                    "another modal dialog is already pending",
                ));
            }
            state.active = Some(ActiveDialog {
                pending: pending.clone(),
                child_dialog_id,
                response: Some(response),
            });
        }
        let cleanup = PendingDialogCleanup {
            control: self.clone(),
            id: pending.id.clone(),
        };
        let resolution = receiver.await.map_err(|_| NativeEngineError::Worker {
            operation: "resume native dialog".into(),
            reason: "native dialog was abandoned before a response arrived".into(),
        })?;
        Ok(NativeDialogWait {
            resolution,
            cleanup,
        })
    }

    pub(crate) fn resolve(
        &self,
        id: &str,
        accepted: bool,
        prompt_value: Option<String>,
    ) -> Result<u64, NativeEngineError> {
        let mut state = self.0.state.lock().map_err(|_| NativeEngineError::Worker {
            operation: "resolve native dialog".into(),
            reason: "native dialog control state is unavailable".into(),
        })?;
        let Some(active) = state.active.as_mut() else {
            return Err(NativeEngineError::invalid(
                "native dialog ID",
                "there is no pending dialog",
            ));
        };
        if active.pending.id != id {
            return Err(NativeEngineError::invalid(
                "native dialog ID",
                "does not match the pending dialog",
            ));
        }
        if active.response.is_none() {
            return Err(NativeEngineError::invalid(
                "native dialog ID",
                "this dialog has already been resolved",
            ));
        }
        let dialog_type = active.pending.dialog.dialog_type.as_str();
        let prompt_value = match (dialog_type, accepted, prompt_value) {
            ("alert" | "confirm", _, None) => None,
            ("alert" | "confirm", _, Some(_)) => {
                return Err(NativeEngineError::invalid(
                    "native prompt response",
                    "only prompt dialogs accept response text",
                ));
            }
            ("prompt", false, None) => None,
            ("prompt", false, Some(_)) => {
                return Err(NativeEngineError::invalid(
                    "native prompt response",
                    "dismissed prompts cannot include response text",
                ));
            }
            ("prompt", true, prompt_value) => Some(
                prompt_value
                    .or(active.pending.dialog.default_value.clone())
                    .unwrap_or_default(),
            ),
            (_, _, _) => {
                return Err(NativeEngineError::invalid(
                    "native dialog type",
                    "must be alert, confirm, or prompt",
                ));
            }
        };
        if let Some(value) = &prompt_value
            && value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
        {
            return Err(NativeEngineError::limit(
                "native prompt response",
                MAX_NATIVE_DIALOG_TEXT_BYTES,
                value.len(),
            ));
        }
        let accepted = if dialog_type == "alert" {
            true
        } else {
            accepted
        };
        let child_dialog_id = active.child_dialog_id;
        let response = active.response.take().expect("dialog response was checked");
        drop(state);
        if response
            .send(NativeDialogResolution {
                accepted,
                prompt_value,
            })
            .is_err()
        {
            self.clear_if_pending(id);
            return Err(NativeEngineError::Worker {
                operation: "resolve native dialog".into(),
                reason: "the suspended content process is no longer waiting for this dialog".into(),
            });
        }
        Ok(child_dialog_id)
    }

    fn clear_if_pending(&self, id: &str) {
        if let Ok(mut state) = self.0.state.lock()
            && state
                .active
                .as_ref()
                .is_some_and(|active| active.pending.id == id)
        {
            state.active = None;
        }
    }
}

struct PendingDialogCleanup {
    control: NativeDialogControlPlane,
    id: String,
}

impl Drop for PendingDialogCleanup {
    fn drop(&mut self) {
        self.control.clear_if_pending(&self.id);
    }
}

fn validate_dialog(dialog: &NativeDialog) -> Result<(), NativeEngineError> {
    if !matches!(dialog.dialog_type.as_str(), "alert" | "confirm" | "prompt") {
        return Err(NativeEngineError::invalid(
            "native dialog type",
            "must be alert, confirm, or prompt",
        ));
    }
    if dialog.message.len() > MAX_NATIVE_DIALOG_TEXT_BYTES
        || dialog
            .default_value
            .as_ref()
            .is_some_and(|value| value.len() > MAX_NATIVE_DIALOG_TEXT_BYTES)
    {
        return Err(NativeEngineError::limit(
            "native dialog text",
            MAX_NATIVE_DIALOG_TEXT_BYTES,
            dialog
                .default_value
                .as_ref()
                .map_or(dialog.message.len(), |value| {
                    dialog.message.len().max(value.len())
                }),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dialog(dialog_type: &str, default_value: Option<&str>) -> NativeDialog {
        NativeDialog {
            dialog_type: dialog_type.into(),
            message: "message".into(),
            default_value: default_value.map(str::to_owned),
        }
    }

    #[tokio::test]
    async fn public_controller_resolves_only_the_current_modal_identity() {
        let control = NativeDialogControlPlane::for_modal_owner();
        let controller = NativeDialogController::new(control.clone()).unwrap();
        let waiting_control = control.clone();
        let waiting = tokio::spawn(async move {
            waiting_control
                .wait_for_resolution(
                    "target-a",
                    "frame-a",
                    "https://example.test/",
                    7,
                    dialog("prompt", Some("seed")),
                )
                .await
        });
        tokio::task::yield_now().await;

        let pending = controller
            .pending_dialog()
            .unwrap()
            .expect("public controller should observe the modal prompt");
        assert_eq!(pending.context_id, "target-a");
        assert_eq!(pending.frame_id, "frame-a");
        assert_eq!(pending.dialog.default_value.as_deref(), Some("seed"));
        assert!(
            controller
                .resolve_dialog(
                    "stale-dialog",
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: Some("wrong".into()),
                    },
                )
                .is_err()
        );
        assert_eq!(
            controller.pending_dialog().unwrap().as_ref(),
            Some(&pending)
        );

        controller
            .resolve_dialog(
                &pending.id,
                NativeDialogResolution {
                    accepted: true,
                    prompt_value: Some("answer".into()),
                },
            )
            .unwrap();
        assert!(
            controller
                .resolve_dialog(
                    &pending.id,
                    NativeDialogResolution {
                        accepted: true,
                        prompt_value: Some("late".into()),
                    },
                )
                .is_err()
        );
        let resolved = waiting.await.unwrap().unwrap();
        assert_eq!(resolved.resolution.prompt_value.as_deref(), Some("answer"));
        resolved.finish();
        assert!(controller.pending_dialog().unwrap().is_none());
    }

    #[test]
    fn public_controller_rejects_a_non_modal_control_plane() {
        assert!(NativeDialogController::new(NativeDialogControlPlane::default()).is_err());
    }

    #[tokio::test]
    async fn dialog_resolution_is_identity_bound_and_injects_prompt_text() {
        let control = NativeDialogControlPlane::default();
        let waiting_control = control.clone();
        let waiting = tokio::spawn(async move {
            waiting_control
                .wait_for_resolution(
                    "target-a",
                    "frame-a",
                    "https://example.test/",
                    7,
                    dialog("prompt", Some("seed")),
                )
                .await
        });
        tokio::task::yield_now().await;
        let pending = control
            .pending()
            .unwrap()
            .expect("dialog should be pending");
        assert_eq!(pending.context_id, "target-a");
        assert_eq!(pending.frame_id, "frame-a");
        assert_eq!(pending.dialog.default_value.as_deref(), Some("seed"));
        assert!(control.resolve("stale-dialog", true, None).is_err());
        assert_eq!(control.pending().unwrap().as_ref(), Some(&pending));

        assert_eq!(
            control
                .resolve(&pending.id, true, Some("answer".into()))
                .unwrap(),
            7
        );
        assert!(control.pending().unwrap().is_none());
        assert!(
            control
                .resolve(&pending.id, true, Some("late".into()))
                .is_err()
        );
        let resolved = waiting.await.unwrap().unwrap();
        assert_eq!(
            resolved.resolution,
            NativeDialogResolution {
                accepted: true,
                prompt_value: Some("answer".into()),
            }
        );
        resolved.finish();
        assert!(control.pending().unwrap().is_none());
    }

    #[tokio::test]
    async fn cancelled_dialog_wait_clears_its_pending_identity() {
        let control = NativeDialogControlPlane::default();
        let waiting_control = control.clone();
        let waiting = tokio::spawn(async move {
            waiting_control
                .wait_for_resolution(
                    "target",
                    "frame",
                    "https://example.test/",
                    1,
                    dialog("confirm", None),
                )
                .await
        });
        tokio::task::yield_now().await;
        let pending = control.pending().unwrap().unwrap();
        waiting.abort();
        let _ = waiting.await;
        assert!(control.pending().unwrap().is_none());
        assert!(control.resolve(&pending.id, true, None).is_err());
    }

    #[tokio::test]
    async fn dismissed_prompt_returns_null_and_accept_without_text_uses_default() {
        for (accepted, expected) in [(false, None), (true, Some("default".to_owned()))] {
            let control = NativeDialogControlPlane::default();
            let waiting_control = control.clone();
            let waiting = tokio::spawn(async move {
                waiting_control
                    .wait_for_resolution(
                        "target",
                        "frame",
                        "about:blank",
                        1,
                        dialog("prompt", Some("default")),
                    )
                    .await
            });
            tokio::task::yield_now().await;
            let pending = control.pending().unwrap().unwrap();
            control.resolve(&pending.id, accepted, None).unwrap();
            let resolution = waiting.await.unwrap().unwrap();
            assert_eq!(resolution.resolution.prompt_value, expected);
            resolution.finish();
        }
    }
}
