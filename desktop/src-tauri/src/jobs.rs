//! Background jobs: registry, cancellation, progress and completion events.
//!
//! A job runs on its own thread. It reports through `job://progress` and finishes with
//! exactly one `job://done`. Only one job per kind runs at a time (`Busy` otherwise).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{IpcError, IpcResult};
use crate::state::{AppState, JobEntry, LockExt};

pub type JobId = String;

/// Why a job stopped early.
#[derive(Debug)]
pub enum JobFail {
    Cancelled,
    Error(IpcError),
}

impl From<IpcError> for JobFail {
    fn from(e: IpcError) -> Self {
        JobFail::Error(e)
    }
}

pub type JobResult = Result<JobOutcome, JobFail>;

/// Successful (or partially successful) end of a job.
#[derive(Debug)]
pub struct JobOutcome {
    pub ok: bool,
    pub summary: String,
    pub payload: Option<Value>,
    /// Set together with `ok = false` to tell the UI what went wrong.
    pub error: Option<IpcError>,
}

impl JobOutcome {
    pub fn ok(summary: impl Into<String>) -> Self {
        Self {
            ok: true,
            summary: summary.into(),
            payload: None,
            error: None,
        }
    }
    pub fn with_payload(mut self, payload: Value) -> Self {
        self.payload = Some(payload);
        self
    }
    pub fn failed(summary: impl Into<String>, error: IpcError) -> Self {
        Self {
            ok: false,
            summary: summary.into(),
            payload: None,
            error: Some(error),
        }
    }
}

/// Handle given to the job closure.
pub struct JobCtx {
    pub id: JobId,
    pub kind: String,
    pub cancel: Arc<AtomicBool>,
    state: AppState,
}

impl JobCtx {
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// `Err(Cancelled)` once the user cancelled.
    pub fn check(&self) -> Result<(), JobFail> {
        if self.is_cancelled() {
            Err(JobFail::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn state(&self) -> &AppState {
        &self.state
    }

    pub fn progress(&self, done: u64, total: u64, message: impl Into<String>) {
        self.emit_progress(done, total, message.into(), None);
    }

    /// Progress with a per-item result (`item = { id, ok, message }`).
    pub fn item(
        &self,
        done: u64,
        total: u64,
        id: impl Into<String>,
        ok: bool,
        message: impl Into<String>,
    ) {
        let message = message.into();
        let item = json!({ "id": id.into(), "ok": ok, "message": message });
        self.emit_progress(done, total, message, Some(item));
    }

    /// Like [`item`](Self::item) with a machine readable failure `reason`.
    pub fn item_reason(
        &self,
        done: u64,
        total: u64,
        id: impl Into<String>,
        ok: bool,
        message: impl Into<String>,
        reason: Option<&str>,
    ) {
        let message = message.into();
        let item = json!({ "id": id.into(), "ok": ok, "message": message, "reason": reason });
        self.emit_progress(done, total, message, Some(item));
    }

    fn emit_progress(&self, done: u64, total: u64, message: String, item: Option<Value>) {
        if let Some(e) = self.state.jobs.lock_safe().get_mut(&self.id) {
            e.done = done;
            e.total = total;
        }
        let mut payload = json!({
            "jobId": self.id,
            "kind": self.kind,
            "done": done,
            "total": total,
            "message": message,
        });
        if let (Some(item), Some(obj)) = (item, payload.as_object_mut()) {
            obj.insert("item".into(), item);
        }
        self.state.emit("job://progress", payload);
    }
}

impl AppState {
    /// Start `f` on a worker thread. The vault must be unlocked; one job per kind.
    pub fn start_job<F>(&self, kind: &str, f: F) -> IpcResult<JobId>
    where
        F: FnOnce(&JobCtx) -> JobResult + Send + 'static,
    {
        if !self.is_unlocked() {
            return Err(IpcError::locked());
        }
        let id = Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut jobs = self.jobs.lock_safe();
            let group = exclusion_group(kind);
            if jobs.values().any(|j| exclusion_group(&j.kind) == group) {
                return Err(IpcError::busy("Dieser Vorgang läuft bereits"));
            }
            jobs.insert(
                id.clone(),
                JobEntry {
                    kind: kind.to_string(),
                    cancel: cancel.clone(),
                    done: 0,
                    total: 0,
                },
            );
        }
        let ctx = JobCtx {
            id: id.clone(),
            kind: kind.to_string(),
            cancel,
            state: self.clone(),
        };
        let spawned = std::thread::Builder::new()
            .name(format!("job-{kind}"))
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&ctx)))
                    .unwrap_or_else(|_| {
                        Err(JobFail::Error(IpcError::internal(
                            "Interner Fehler im Hintergrundvorgang",
                        )))
                    });
                ctx.state.jobs.lock_safe().remove(&ctx.id);
                let payload = done_payload(&ctx.id, &ctx.kind, result);
                ctx.state.emit("job://done", payload);
            });
        if let Err(e) = spawned {
            self.jobs.lock_safe().remove(&id);
            return Err(IpcError::internal(format!(
                "Hintergrundvorgang konnte nicht gestartet werden: {e}"
            )));
        }
        Ok(id)
    }

    pub fn cancel_job(&self, id: &str) -> IpcResult<()> {
        match self.jobs.lock_safe().get(id) {
            Some(j) => {
                j.cancel.store(true, Ordering::SeqCst);
                Ok(())
            }
            None => Err(IpcError::not_found("Vorgang")),
        }
    }

    pub fn cancel_all_jobs(&self) {
        for j in self.jobs.lock_safe().values() {
            j.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn has_running_jobs(&self) -> bool {
        !self.jobs.lock_safe().is_empty()
    }

    pub fn job_running(&self, kind: &str) -> bool {
        self.jobs.lock_safe().values().any(|j| j.kind == kind)
    }

    pub fn jobs_running(&self) -> Vec<Value> {
        self.jobs
            .lock_safe()
            .iter()
            .map(|(id, j)| json!({ "jobId": id, "kind": j.kind, "done": j.done, "total": j.total }))
            .collect()
    }
}

/// Jobs of one group never run at the same time. Everything that can send mail shares
/// the group `send`, so two jobs can never contact the same account twice.
fn exclusion_group(kind: &str) -> &str {
    match kind {
        "automation" => "send",
        other => other,
    }
}

fn done_payload(id: &str, kind: &str, result: JobResult) -> Value {
    match result {
        Ok(o) => {
            let mut v = json!({
                "jobId": id, "kind": kind, "ok": o.ok, "summary": o.summary,
            });
            if let Some(obj) = v.as_object_mut() {
                if let Some(e) = o.error {
                    obj.insert("error".into(), json!(e));
                }
                if let Some(p) = o.payload {
                    obj.insert("payload".into(), p);
                }
            }
            v
        }
        Err(JobFail::Cancelled) => json!({
            "jobId": id, "kind": kind, "ok": false, "summary": "Abgebrochen",
        }),
        Err(JobFail::Error(e)) => json!({
            "jobId": id, "kind": kind, "ok": false, "summary": e.message, "error": e,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;
    use crate::state::testing::unlocked;
    use std::time::Duration;

    #[test]
    fn job_runs_reports_progress_and_done() {
        let f = unlocked();
        let id = f
            .state
            .start_job("test", |ctx| {
                ctx.progress(1, 2, "halb");
                ctx.item(2, 2, "x", true, "fertig");
                Ok(JobOutcome::ok("alles gut").with_payload(json!({ "n": 2 })))
            })
            .unwrap();
        let done = f.host.wait_for("job://done", 5).expect("done event");
        assert_eq!(done["jobId"], id);
        assert_eq!(done["ok"], true);
        assert_eq!(done["payload"]["n"], 2);
        let progress = f.host.named("job://progress");
        assert_eq!(progress.len(), 2);
        assert_eq!(progress[1]["item"]["id"], "x");
        assert!(f.state.jobs_running().is_empty());
    }

    #[test]
    fn cancel_stops_a_running_job() {
        let f = unlocked();
        let id = f
            .state
            .start_job("loop", |ctx| loop {
                ctx.check()?;
                std::thread::sleep(Duration::from_millis(10));
            })
            .unwrap();
        assert_eq!(f.state.jobs_running().len(), 1);
        f.state.cancel_job(&id).unwrap();
        let done = f.host.wait_for("job://done", 5).expect("done event");
        assert_eq!(done["ok"], false);
        assert_eq!(done["summary"], "Abgebrochen");
        assert!(done.get("error").is_none());
        assert_eq!(
            f.state.cancel_job(&id).unwrap_err().code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn second_job_of_same_kind_is_busy() {
        let f = unlocked();
        f.state
            .start_job("scan", |ctx| {
                while !ctx.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(JobFail::Cancelled)
            })
            .unwrap();
        let e = f
            .state
            .start_job("scan", |_| Ok(JobOutcome::ok("x")))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Busy);
        // a different kind is fine
        f.state
            .start_job("other", |_| Ok(JobOutcome::ok("x")))
            .unwrap();
        f.state.cancel_all_jobs();
    }

    #[test]
    fn panic_in_job_becomes_internal_error() {
        let f = unlocked();
        f.state
            .start_job("boom", |_| -> JobResult { panic!("kaputt") })
            .unwrap();
        let done = f.host.wait_for("job://done", 5).unwrap();
        assert_eq!(done["ok"], false);
        assert_eq!(done["error"]["code"], "Internal");
        assert!(f.state.jobs_running().is_empty());
    }

    #[test]
    fn locking_cancels_jobs_and_locked_vault_rejects_new_ones() {
        let f = unlocked();
        f.state
            .start_job("loop", |ctx| loop {
                ctx.check()?;
                std::thread::sleep(Duration::from_millis(10));
            })
            .unwrap();
        f.state.lock();
        assert!(f.host.wait_for("job://done", 5).is_some());
        let e = f
            .state
            .start_job("x", |_| Ok(JobOutcome::ok("x")))
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::VaultLocked);
    }
}
