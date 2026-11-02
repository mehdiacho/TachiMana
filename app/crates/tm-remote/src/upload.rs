//! Upload queue (new in the Rust port, PLAN §P6.3): after a download finishes and NAS storage is
//! on, the chapter is packed as a CBZ and uploaded under Mihon's names, the remote size is checked
//! against the local file, and only then is it marked uploaded. With the archive role the local
//! copy is deleted after a verified upload; with the mirror role both are kept. The queue is saved
//! to disk so it survives a restart.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::index::RemoteIndex;
use crate::webdav::WebDav;
use crate::{Error, Result};

/// Most pages packed from one chapter, and most bytes.
const MAX_PAGES: usize = 5000;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Most jobs kept (finished ones are dropped first).
const MAX_JOBS: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Keep the local copy too.
    Mirror,
    /// Delete the local copy once the upload is verified.
    Archive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UploadStatus {
    Queued,
    Uploading,
    Done,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadJob {
    pub id: u64,
    /// The downloaded chapter folder.
    pub local_dir: PathBuf,
    /// `<source dir>/<manga dir>/<chapter>.cbz` on the NAS.
    pub segments: Vec<String>,
    pub role: Role,
    pub status: UploadStatus,
    pub error: Option<String>,
    /// Shown in the UI.
    pub label: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    next_id: u64,
    jobs: Vec<UploadJob>,
}

#[derive(Debug)]
pub struct UploadQueue {
    file: PathBuf,
    /// Only folders under here are ever packed or deleted, whatever the saved queue says.
    root: PathBuf,
    state: Mutex<Saved>,
}

impl UploadQueue {
    /// Loads the queue saved at `file`. Jobs cut off mid-upload are queued again. `root` is the
    /// downloads folder.
    pub fn open(file: impl Into<PathBuf>, root: impl Into<PathBuf>) -> Self {
        let file = file.into();
        let mut saved: Saved = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for j in &mut saved.jobs {
            if j.status == UploadStatus::Uploading {
                j.status = UploadStatus::Queued;
            }
        }
        if let Some(max) = saved.jobs.iter().map(|j| j.id).max() {
            saved.next_id = saved.next_id.max(max.saturating_add(1));
        }
        Self {
            file,
            root: root.into(),
            state: Mutex::new(saved),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Saved) -> T) -> T {
        let mut g = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut g)
    }

    fn save(&self) {
        let body = self.with(|s| serde_json::to_vec_pretty(&*s));
        let write = |body: Vec<u8>| -> std::io::Result<()> {
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let part = self.file.with_extension("json.part");
            std::fs::write(&part, body)?;
            std::fs::rename(&part, &self.file)
        };
        match body {
            Ok(b) => {
                if let Err(e) = write(b) {
                    tracing::warn!("upload queue save failed: {e}");
                }
            }
            Err(e) => tracing::warn!("upload queue encode failed: {e}"),
        }
    }

    /// Queue a chapter. Returns false when the same upload is already waiting or running.
    pub fn enqueue(
        &self,
        local_dir: PathBuf,
        segments: Vec<String>,
        role: Role,
        label: String,
    ) -> bool {
        let added = self.with(|s| {
            let busy = s.jobs.iter().any(|j| {
                j.segments == segments
                    && matches!(j.status, UploadStatus::Queued | UploadStatus::Uploading)
            });
            if busy {
                return false;
            }
            if s.jobs.len() >= MAX_JOBS {
                s.jobs
                    .retain(|j| !matches!(j.status, UploadStatus::Done | UploadStatus::Failed));
                if s.jobs.len() >= MAX_JOBS {
                    return false;
                }
            }
            let id = s.next_id;
            s.next_id = s.next_id.saturating_add(1);
            s.jobs.push(UploadJob {
                id,
                local_dir,
                segments,
                role,
                status: UploadStatus::Queued,
                error: None,
                label,
            });
            true
        });
        if added {
            self.save();
        }
        added
    }

    pub fn jobs(&self) -> Vec<UploadJob> {
        self.with(|s| s.jobs.clone())
    }

    pub fn has_queued(&self) -> bool {
        self.with(|s| s.jobs.iter().any(|j| j.status == UploadStatus::Queued))
    }

    /// Remove a job that is not running.
    pub fn dismiss(&self, id: u64) {
        self.with(|s| {
            s.jobs
                .retain(|j| j.id != id || j.status == UploadStatus::Uploading)
        });
        self.save();
    }

    /// Queue failed jobs again.
    pub fn retry_failed(&self) {
        self.with(|s| {
            for j in s
                .jobs
                .iter_mut()
                .filter(|j| j.status == UploadStatus::Failed)
            {
                j.status = UploadStatus::Queued;
                j.error = None;
            }
        });
        self.save();
    }

    pub fn clear_finished(&self) {
        self.with(|s| s.jobs.retain(|j| j.status != UploadStatus::Done));
        self.save();
    }

    fn set(&self, id: u64, status: UploadStatus, error: Option<String>) {
        self.with(|s| {
            if let Some(j) = s.jobs.iter_mut().find(|j| j.id == id) {
                j.status = status;
                j.error = error;
            }
        });
        self.save();
    }

    /// Runs the oldest queued job to the end. `None` when nothing is queued.
    pub async fn run_next(&self, dav: &WebDav, index: &RemoteIndex) -> Option<UploadJob> {
        let job = self.with(|s| {
            let j = s
                .jobs
                .iter_mut()
                .find(|j| j.status == UploadStatus::Queued)?;
            j.status = UploadStatus::Uploading;
            Some(j.clone())
        })?;
        self.save();
        let result = self.upload(&job, dav).await;
        match result {
            Ok(()) => {
                index.on_uploaded(&job.segments);
                if job.role == Role::Archive {
                    if let Err(e) = std::fs::remove_dir_all(&job.local_dir) {
                        tracing::warn!(
                            "couldn't remove {} after upload: {e}",
                            job.local_dir.display()
                        );
                    }
                }
                self.set(job.id, UploadStatus::Done, None);
            }
            Err(e) => self.set(job.id, UploadStatus::Failed, Some(e.to_string())),
        }
        self.with(|s| s.jobs.iter().find(|j| j.id == job.id).cloned())
    }

    /// The chapter folder, only if it really lies under the downloads root.
    fn checked_dir(&self, dir: &Path) -> Result<PathBuf> {
        let io = |e| Error::Io(dir.display().to_string(), e);
        let real = std::fs::canonicalize(dir).map_err(io)?;
        let root = std::fs::canonicalize(&self.root)
            .map_err(|e| Error::Io(self.root.display().to_string(), e))?;
        if real.starts_with(&root) && real != root {
            Ok(real)
        } else {
            Err(Error::Config(format!(
                "{} is not inside the downloads folder",
                dir.display()
            )))
        }
    }

    async fn upload(&self, job: &UploadJob, dav: &WebDav) -> Result<()> {
        if job.segments.len() != 3 {
            return Err(Error::Config(
                "An upload needs <source>/<manga>/<file>".into(),
            ));
        }
        let dir = self.checked_dir(&job.local_dir)?;
        let names = tm_archive::page_names(&dir)?;
        if names.is_empty() || names.len() > MAX_PAGES {
            return Err(Error::Config(format!(
                "{} has {} pages",
                dir.display(),
                names.len()
            )));
        }
        let mut pages = Vec::with_capacity(names.len());
        let mut total = 0u64;
        for n in names {
            let bytes = tm_archive::read_page(&dir, &n)?;
            total = total.saturating_add(bytes.len() as u64);
            if total > MAX_BYTES {
                return Err(Error::Config("The chapter is too large to upload".into()));
            }
            pages.push((n, bytes));
        }
        let work = self.file.with_file_name(format!("upload-{}.cbz", job.id));
        tm_archive::pack_cbz(&pages, &work)?;
        drop(pages);
        let outcome = async {
            let local = std::fs::metadata(&work)
                .map_err(|e| Error::Io(work.display().to_string(), e))?
                .len();
            dav.put_file(&job.segments, &work).await?;
            match dav.size_of(&job.segments).await? {
                Some(Some(remote)) if remote == local => Ok(()),
                Some(Some(remote)) => Err(Error::Server(format!(
                    "The server holds {remote} bytes, expected {local}; the local copy was kept"
                ))),
                Some(None) => Err(Error::Server(
                    "The server did not report the uploaded size; the local copy was kept".into(),
                )),
                None => Err(Error::Server("The upload is missing on the server".into())),
            }
        }
        .await;
        let _ = std::fs::remove_file(&work);
        outcome
    }
}
