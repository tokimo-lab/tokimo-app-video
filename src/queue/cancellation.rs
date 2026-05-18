use tokio_util::sync::CancellationToken;
use crate::error::AppError;

pub type JobCancel = CancellationToken;

pub fn new_cancel() -> JobCancel {
    CancellationToken::new()
}

pub fn check_cancel(cancel: &JobCancel) -> Result<(), AppError> {
    if cancel.is_cancelled() {
        Err(AppError::Gone("job cancelled".into()))
    } else {
        Ok(())
    }
}

pub const CANCEL_MARKER_ABORTED: &str = "aborted";
