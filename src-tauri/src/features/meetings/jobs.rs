// FilePath: src-tauri/src/features/meetings/jobs.rs
//! One long job (processing, summary, answer, translation) per meeting at a time, and its
//! cancellation: cancelling drops the job's future, which cancels the engine request it waits on.

use std::future::Future;
use std::sync::atomic::Ordering;

use sv_domain::{AppError, AppResult, MeetingJob};
use tauri::{AppHandle, Manager};
use tokio::sync::oneshot;

use super::{MeetingsState, CANCELLED};
use crate::state::lock;

/// A running job; sending on `cancel` ends it.
pub(super) struct Job {
    token: u64,
    pub(super) kind: MeetingJob,
    cancel: Option<oneshot::Sender<()>>,
}

/// Removes its job from [`MeetingsState`] when the job ends, however it ends.
pub(super) struct JobGuard {
    app: AppHandle,
    meeting: i64,
    token: u64,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        let meetings = self.app.state::<MeetingsState>();
        let mut jobs = lock(&meetings.jobs);
        if jobs
            .get(&self.meeting)
            .is_some_and(|job| job.token == self.token)
        {
            jobs.remove(&self.meeting);
        }
    }
}

/// Registers a job for `meeting`; one runs per meeting at a time.
pub(super) fn begin_job(
    app: &AppHandle,
    meeting: i64,
    kind: MeetingJob,
) -> AppResult<(JobGuard, oneshot::Receiver<()>)> {
    let meetings = app.state::<MeetingsState>();
    let (cancel, cancelled) = oneshot::channel();
    let token = {
        let mut jobs = lock(&meetings.jobs);
        if let Some(job) = jobs.get(&meeting) {
            return Err(AppError::invalid(format!(
                "This meeting is busy: {} is still running.",
                job_name(job.kind)
            )));
        }
        let token = meetings.next_job.fetch_add(1, Ordering::Relaxed);
        jobs.insert(
            meeting,
            Job {
                token,
                kind,
                cancel: Some(cancel),
            },
        );
        token
    };
    let guard = JobGuard {
        app: app.clone(),
        meeting,
        token,
    };
    Ok((guard, cancelled))
}

pub(super) fn job_name(job: MeetingJob) -> &'static str {
    match job {
        MeetingJob::Transcribe | MeetingJob::Diarize => "processing the recording",
        MeetingJob::Summarize => "the summary",
        MeetingJob::Chat => "an answer",
        MeetingJob::Translate => "a translation",
    }
}

/// Runs `work` until it finishes or the job is cancelled. Cancelling drops `work`, which
/// cancels the engine request it is waiting on.
pub(super) async fn cancellable<T>(
    cancelled: oneshot::Receiver<()>,
    work: impl Future<Output = AppResult<T>>,
) -> AppResult<T> {
    tokio::select! {
        biased;
        _ = cancelled => Err(AppError::Other(CANCELLED.to_owned())),
        result = work => result,
    }
}

pub(super) fn is_cancelled(error: &AppError) -> bool {
    matches!(error, AppError::Other(message) if message == CANCELLED)
}

/// Asks the meeting's running job to stop; returns whether one was running.
pub(super) fn cancel_job(app: &AppHandle, meeting: i64) -> bool {
    let meetings = app.state::<MeetingsState>();
    let mut jobs = lock(&meetings.jobs);
    match jobs.get_mut(&meeting).and_then(|job| job.cancel.take()) {
        Some(cancel) => {
            // The job may have just finished; then there is nothing left to stop.
            let _ = cancel.send(());
            true
        }
        None => false,
    }
}
