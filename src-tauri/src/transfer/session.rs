//! Receive-side protocol state machine.
//!
//! `ReceiveSession` decides whether each incoming message is allowed in the
//! current state and keeps the bookkeeping needed to verify what was written.
//! It does no IO, so the rules can be unit-tested without Tauri or QUIC.

use crate::transfer::protocol::{FileEntry, FileMetadata, TransferOutcome, MAX_CHUNK_SIZE};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct ProtocolError(pub String);

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Protocol violation: {}", self.0)
    }
}

impl std::error::Error for ProtocolError {}

fn violation<T>(msg: impl Into<String>) -> Result<T, ProtocolError> {
    Err(ProtocolError(msg.into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    AwaitingOffer,
    AwaitingDecision,
    Receiving,
    Done,
}

#[derive(Debug, Clone)]
pub struct Offer {
    pub transfer_id: String,
    pub sender_id: String,
    /// Sanitized single path segment the transfer is saved under.
    pub name: String,
    pub is_dir: bool,
    pub total_size: u64,
}

struct CurrentFile {
    path: PathBuf,
    declared: u64,
    received: u64,
    hasher: blake3::Hasher,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVerdict {
    Verified,
    Corrupt(String),
}

#[derive(Debug, Clone)]
pub struct FileResult {
    /// Path relative to the save directory.
    pub path: PathBuf,
    pub received: u64,
    pub verdict: FileVerdict,
}

pub struct ReceiveSession {
    phase: Phase,
    offer: Option<Offer>,
    /// Expected files keyed by their sanitized path relative to the transfer root.
    manifest: Option<HashMap<PathBuf, u64>>,
    started: HashSet<PathBuf>,
    current: Option<CurrentFile>,
    verified: usize,
    failed: usize,
    bytes_received: u64,
}

impl Default for ReceiveSession {
    fn default() -> Self {
        Self::new()
    }
}

impl ReceiveSession {
    pub fn new() -> Self {
        Self {
            phase: Phase::AwaitingOffer,
            offer: None,
            manifest: None,
            started: HashSet::new(),
            current: None,
            verified: 0,
            failed: 0,
            bytes_received: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn offer(&self) -> Option<&Offer> {
        self.offer.as_ref()
    }

    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
    }

    /// Path (relative to the save directory) of the file currently being written.
    pub fn current_path(&self) -> Option<&Path> {
        self.current.as_ref().map(|c| c.path.as_path())
    }

    pub fn current_progress(&self) -> Option<(u64, u64)> {
        self.current.as_ref().map(|c| (c.received, c.declared))
    }

    fn check_id(&self, transfer_id: &str) -> Result<(), ProtocolError> {
        match &self.offer {
            Some(offer) if offer.transfer_id == transfer_id => Ok(()),
            _ => violation(format!("unknown transfer id '{}'", transfer_id)),
        }
    }

    fn require_receiving(&self, what: &str) -> Result<(), ProtocolError> {
        if self.phase != Phase::Receiving {
            return violation(format!("{} before the offer was accepted", what));
        }
        Ok(())
    }

    /// Validates an offer. The caller is responsible for the trust check.
    pub fn on_offer(
        &mut self,
        transfer_id: &str,
        sender_id: &str,
        metadata: &FileMetadata,
    ) -> Result<&Offer, ProtocolError> {
        if self.phase != Phase::AwaitingOffer {
            return violation("unexpected second offer");
        }
        if transfer_id.is_empty() {
            return violation("empty transfer id");
        }
        let name = match sanitize_file_name(&metadata.name) {
            Some(n) => n,
            None => return violation(format!("invalid file name '{}'", metadata.name)),
        };

        self.offer = Some(Offer {
            transfer_id: transfer_id.to_string(),
            sender_id: sender_id.to_string(),
            name,
            is_dir: metadata.is_dir,
            total_size: metadata.size,
        });
        self.phase = Phase::AwaitingDecision;
        Ok(self.offer.as_ref().unwrap())
    }

    /// Applies a user-chosen name before accepting. Must be a single, safe segment.
    pub fn rename(&mut self, new_name: &str) -> Result<(), ProtocolError> {
        if self.phase != Phase::AwaitingDecision {
            return violation("rename outside of the decision phase");
        }
        let name = match sanitize_file_name(new_name) {
            Some(n) => n,
            None => return violation(format!("invalid file name '{}'", new_name)),
        };
        if let Some(offer) = &mut self.offer {
            offer.name = name;
        }
        Ok(())
    }

    pub fn on_accept(&mut self) -> Result<(), ProtocolError> {
        if self.phase != Phase::AwaitingDecision {
            return violation("accept outside of the decision phase");
        }
        let offer = self.offer.as_ref().unwrap();
        if !offer.is_dir {
            // A single file is a one-entry manifest keyed by the empty path.
            let mut manifest = HashMap::new();
            manifest.insert(PathBuf::new(), offer.total_size);
            self.manifest = Some(manifest);
        }
        self.phase = Phase::Receiving;
        Ok(())
    }

    pub fn on_reject(&mut self) {
        self.phase = Phase::Done;
    }

    /// Validates a folder manifest. Returns the sanitized paths relative to the
    /// save directory so the caller can create the folder structure.
    pub fn on_manifest(
        &mut self,
        transfer_id: &str,
        files: &[FileEntry],
    ) -> Result<Vec<PathBuf>, ProtocolError> {
        self.require_receiving("manifest")?;
        self.check_id(transfer_id)?;
        let offer = self.offer.as_ref().unwrap();
        if !offer.is_dir {
            return violation("manifest for a single-file transfer");
        }
        if self.manifest.is_some() {
            return violation("duplicate manifest");
        }

        let mut manifest = HashMap::with_capacity(files.len());
        let mut total: u64 = 0;
        for entry in files {
            let rel = match sanitize_relative_path(&entry.relative_path) {
                Some(p) => p,
                None => return violation(format!("unsafe path '{}'", entry.relative_path)),
            };
            if manifest.insert(rel, entry.size).is_some() {
                return violation(format!("duplicate path '{}'", entry.relative_path));
            }
            total = match total.checked_add(entry.size) {
                Some(t) => t,
                None => return violation("manifest size overflow"),
            };
        }
        if total > offer.total_size {
            return violation("manifest is larger than the offered size");
        }

        let root = PathBuf::from(&offer.name);
        let paths = manifest.keys().map(|rel| root.join(rel)).collect();
        self.manifest = Some(manifest);
        Ok(paths)
    }

    /// Validates the start of a file. Returns its path relative to the save directory.
    pub fn on_file_start(
        &mut self,
        transfer_id: &str,
        relative_path: &str,
        size: u64,
    ) -> Result<PathBuf, ProtocolError> {
        self.require_receiving("file start")?;
        self.check_id(transfer_id)?;
        if self.current.is_some() {
            return violation("file started before the previous one ended");
        }
        let offer = self.offer.as_ref().unwrap();
        let manifest = match &self.manifest {
            Some(m) => m,
            None => return violation("file started before the manifest"),
        };

        let key = if offer.is_dir {
            match sanitize_relative_path(relative_path) {
                Some(p) => p,
                None => return violation(format!("unsafe path '{}'", relative_path)),
            }
        } else {
            // The sender's name is irrelevant; the receiver decided the name at accept time.
            PathBuf::new()
        };

        match manifest.get(&key) {
            Some(&expected) if expected == size => {}
            Some(_) => return violation(format!("size mismatch for '{}'", relative_path)),
            None => return violation(format!("'{}' is not in the manifest", relative_path)),
        }
        if !self.started.insert(key.clone()) {
            return violation(format!("'{}' sent twice", relative_path));
        }

        let path = if offer.is_dir {
            PathBuf::from(&offer.name).join(key)
        } else {
            PathBuf::from(&offer.name)
        };
        self.current = Some(CurrentFile {
            path: path.clone(),
            declared: size,
            received: 0,
            hasher: blake3::Hasher::new(),
        });
        Ok(path)
    }

    /// Validates a chunk header before its payload is written.
    pub fn on_chunk(&mut self, transfer_id: &str, len: usize) -> Result<(), ProtocolError> {
        self.require_receiving("chunk")?;
        self.check_id(transfer_id)?;
        if len == 0 || len > MAX_CHUNK_SIZE {
            return violation(format!("invalid chunk size {}", len));
        }
        let current = match &self.current {
            Some(c) => c,
            None => return violation("chunk outside of a file"),
        };
        if current.received + len as u64 > current.declared {
            return violation("more data than the declared file size");
        }
        Ok(())
    }

    /// Records chunk bytes that were validated with [`on_chunk`] and written to disk.
    pub fn absorb(&mut self, data: &[u8]) {
        if let Some(current) = &mut self.current {
            current.hasher.update(data);
            current.received += data.len() as u64;
            self.bytes_received += data.len() as u64;
        }
    }

    pub fn on_file_end(
        &mut self,
        transfer_id: &str,
        bytes: u64,
        hash: &str,
    ) -> Result<FileResult, ProtocolError> {
        self.require_receiving("file end")?;
        self.check_id(transfer_id)?;
        let current = match self.current.take() {
            Some(c) => c,
            None => return violation("file end without a file"),
        };

        let actual_hash = current.hasher.finalize().to_hex().to_string();
        let verdict = if current.received != current.declared || bytes != current.received {
            FileVerdict::Corrupt(format!(
                "received {} of {} bytes (sender reported {})",
                current.received, current.declared, bytes
            ))
        } else if actual_hash != hash {
            FileVerdict::Corrupt("hash mismatch".to_string())
        } else {
            FileVerdict::Verified
        };

        match verdict {
            FileVerdict::Verified => self.verified += 1,
            FileVerdict::Corrupt(_) => self.failed += 1,
        }
        Ok(FileResult {
            path: current.path,
            received: current.received,
            verdict,
        })
    }

    /// Ends the transfer. A file still open at this point counts as failed and is
    /// returned so the caller can discard it.
    pub fn on_complete(
        &mut self,
        transfer_id: &str,
    ) -> Result<(TransferOutcome, Option<PathBuf>), ProtocolError> {
        self.require_receiving("completion")?;
        self.check_id(transfer_id)?;

        let unfinished = self.current.take().map(|c| {
            self.failed += 1;
            c.path
        });
        let expected = self.manifest.as_ref().map(|m| m.len()).unwrap_or(0);

        let outcome = if expected > 0 && self.verified == expected && self.failed == 0 {
            TransferOutcome::Completed
        } else if self.verified > 0 {
            TransferOutcome::PartialSuccess
        } else {
            TransferOutcome::Failed
        };
        self.phase = Phase::Done;
        Ok((outcome, unfinished))
    }

    /// Pause/resume/cancel from the sender are only meaningful once data flows
    /// (cancel is also allowed while the user is still deciding).
    pub fn on_remote_control(&self, transfer_id: &str, is_cancel: bool) -> Result<(), ProtocolError> {
        self.check_id(transfer_id)?;
        match self.phase {
            Phase::Receiving => Ok(()),
            Phase::AwaitingDecision if is_cancel => Ok(()),
            _ => violation("transfer control message in the wrong state"),
        }
    }
}

/// Turns a peer-supplied relative path into a safe one. Accepts both `/` and `\`
/// as separators, rejects absolute paths, `..`, drive prefixes (`C:`), NUL bytes
/// and empty paths. Never trust a path from the network without this.
pub fn sanitize_relative_path(raw: &str) -> Option<PathBuf> {
    if raw.starts_with('/') || raw.starts_with('\\') {
        return None;
    }

    let mut out = PathBuf::new();
    for part in raw.split(['/', '\\']) {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." || part.contains(':') || part.contains('\0') {
            return None;
        }
        // Belt and braces: the segment must be exactly one normal component on this OS.
        let mut comps = Path::new(part).components();
        match (comps.next(), comps.next()) {
            (Some(std::path::Component::Normal(_)), None) => out.push(part),
            _ => return None,
        }
    }

    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Like [`sanitize_relative_path`] but requires exactly one segment.
pub fn sanitize_file_name(raw: &str) -> Option<String> {
    let path = sanitize_relative_path(raw)?;
    if path.components().count() != 1 {
        return None;
    }
    path.to_str().map(|s| s.to_string())
}

/// Resolves `rel` (already sanitized) under `base`, creating parent directories.
/// Fails if an existing symlink would redirect the write outside `base`.
pub fn resolve_destination(base: &Path, rel: &Path) -> std::io::Result<PathBuf> {
    let target = base.join(rel);
    let parent = target.parent().unwrap_or(base);
    std::fs::create_dir_all(parent)?;

    let canonical_base = base.canonicalize()?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_base) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "destination escapes the download folder",
        ));
    }
    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "refusing to write through a symlink",
            ));
        }
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(name: &str, size: u64, is_dir: bool) -> FileMetadata {
        FileMetadata {
            name: name.to_string(),
            size,
            is_dir,
            file_count: None,
            subfolder_count: None,
            top_extensions: None,
        }
    }

    fn entry(path: &str, size: u64) -> FileEntry {
        FileEntry {
            relative_path: path.to_string(),
            size,
        }
    }

    fn hash(data: &[u8]) -> String {
        blake3::hash(data).to_hex().to_string()
    }

    fn accepted_file(size: u64) -> ReceiveSession {
        let mut s = ReceiveSession::new();
        s.on_offer("t1", "dev", &meta("a.txt", size, false)).unwrap();
        s.on_accept().unwrap();
        s
    }

    #[test]
    fn rejects_file_start_before_offer() {
        // The 1.0.0 bypass: an empty transfer id matched the empty initial state.
        let mut s = ReceiveSession::new();
        assert!(s.on_file_start("", "evil.txt", 4).is_err());
        assert!(s.on_chunk("", 4).is_err());
        assert!(s.on_complete("").is_err());
    }

    #[test]
    fn rejects_file_start_before_accept() {
        let mut s = ReceiveSession::new();
        s.on_offer("t1", "dev", &meta("a.txt", 4, false)).unwrap();
        assert!(s.on_file_start("t1", "a.txt", 4).is_err());
        assert!(s.on_remote_control("t1", false).is_err());
        assert!(s.on_remote_control("t1", true).is_ok());
    }

    #[test]
    fn rejects_empty_or_mismatched_transfer_id() {
        let mut s = ReceiveSession::new();
        assert!(s.on_offer("", "dev", &meta("a.txt", 4, false)).is_err());
        let mut s = accepted_file(4);
        assert!(s.on_file_start("other", "a.txt", 4).is_err());
        s.on_file_start("t1", "a.txt", 4).unwrap();
        assert!(s.on_chunk("other", 4).is_err());
    }

    #[test]
    fn single_file_happy_path() {
        let mut s = accepted_file(4);
        let path = s.on_file_start("t1", "whatever", 4).unwrap();
        assert_eq!(path, PathBuf::from("a.txt"));
        s.on_chunk("t1", 4).unwrap();
        s.absorb(b"data");
        let r = s.on_file_end("t1", 4, &hash(b"data")).unwrap();
        assert_eq!(r.verdict, FileVerdict::Verified);
        let (outcome, unfinished) = s.on_complete("t1").unwrap();
        assert_eq!(outcome, TransferOutcome::Completed);
        assert!(unfinished.is_none());
    }

    #[test]
    fn rejects_bytes_beyond_declared_size() {
        let mut s = accepted_file(4);
        s.on_file_start("t1", "a.txt", 4).unwrap();
        assert!(s.on_chunk("t1", 5).is_err());
        s.on_chunk("t1", 3).unwrap();
        s.absorb(b"abc");
        assert!(s.on_chunk("t1", 2).is_err());
    }

    #[test]
    fn rejects_oversize_chunk() {
        let mut s = accepted_file(u64::MAX);
        s.on_file_start("t1", "a.txt", u64::MAX).unwrap();
        assert!(s.on_chunk("t1", MAX_CHUNK_SIZE + 1).is_err());
        assert!(s.on_chunk("t1", 0).is_err());
    }

    #[test]
    fn hash_mismatch_fails_the_file() {
        let mut s = accepted_file(4);
        s.on_file_start("t1", "a.txt", 4).unwrap();
        s.on_chunk("t1", 4).unwrap();
        s.absorb(b"data");
        let r = s.on_file_end("t1", 4, &hash(b"nope")).unwrap();
        assert!(matches!(r.verdict, FileVerdict::Corrupt(_)));
        assert_eq!(s.on_complete("t1").unwrap().0, TransferOutcome::Failed);
    }

    #[test]
    fn short_file_fails() {
        let mut s = accepted_file(4);
        s.on_file_start("t1", "a.txt", 4).unwrap();
        s.on_chunk("t1", 2).unwrap();
        s.absorb(b"da");
        let r = s.on_file_end("t1", 2, &hash(b"da")).unwrap();
        assert!(matches!(r.verdict, FileVerdict::Corrupt(_)));
    }

    #[test]
    fn folder_requires_manifest_entries() {
        let mut s = ReceiveSession::new();
        s.on_offer("t1", "dev", &meta("dir", 10, true)).unwrap();
        s.on_accept().unwrap();
        assert!(s.on_file_start("t1", "a.txt", 5).is_err(), "no manifest yet");

        let paths = s
            .on_manifest("t1", &[entry("a.txt", 5), entry("sub\\b.txt", 5)])
            .unwrap();
        assert!(paths.contains(&PathBuf::from("dir").join("sub").join("b.txt")));

        assert!(s.on_file_start("t1", "c.txt", 5).is_err(), "not in manifest");
        assert!(s.on_file_start("t1", "a.txt", 6).is_err(), "wrong size");
        let p = s.on_file_start("t1", "sub/b.txt", 5).unwrap();
        assert_eq!(p, PathBuf::from("dir").join("sub").join("b.txt"));
    }

    #[test]
    fn manifest_cannot_exceed_offer_or_escape() {
        let mut s = ReceiveSession::new();
        s.on_offer("t1", "dev", &meta("dir", 10, true)).unwrap();
        s.on_accept().unwrap();
        assert!(s.on_manifest("t1", &[entry("a", 6), entry("b", 6)]).is_err());
        assert!(s.on_manifest("t1", &[entry("../a", 1)]).is_err());
        assert!(s.on_manifest("t1", &[entry("a", 1), entry("./a", 1)]).is_err());
    }

    #[test]
    fn missing_folder_file_is_partial_success() {
        let mut s = ReceiveSession::new();
        s.on_offer("t1", "dev", &meta("dir", 8, true)).unwrap();
        s.on_accept().unwrap();
        s.on_manifest("t1", &[entry("a", 4), entry("b", 4)]).unwrap();
        s.on_file_start("t1", "a", 4).unwrap();
        s.on_chunk("t1", 4).unwrap();
        s.absorb(b"aaaa");
        s.on_file_end("t1", 4, &hash(b"aaaa")).unwrap();
        assert_eq!(s.on_complete("t1").unwrap().0, TransferOutcome::PartialSuccess);
    }

    #[test]
    fn rename_must_be_a_single_safe_segment() {
        let mut s = ReceiveSession::new();
        s.on_offer("t1", "dev", &meta("a.txt", 1, false)).unwrap();
        assert!(s.rename("../x").is_err());
        assert!(s.rename("sub/x").is_err());
        s.rename("b.txt").unwrap();
        assert_eq!(s.offer().unwrap().name, "b.txt");
    }

    #[test]
    fn sanitize_rejects_traversal_and_absolute_paths() {
        assert_eq!(sanitize_relative_path("../x"), None);
        assert_eq!(sanitize_relative_path("a/../../x"), None);
        assert_eq!(sanitize_relative_path("/etc/passwd"), None);
        assert_eq!(sanitize_relative_path("\\\\server\\share\\x"), None);
        assert_eq!(sanitize_relative_path("C:evil.txt"), None);
        assert_eq!(sanitize_relative_path("C:\\Windows\\x"), None);
        assert_eq!(sanitize_relative_path("a\0b"), None);
        assert_eq!(sanitize_relative_path(""), None);
        assert_eq!(sanitize_relative_path("./"), None);
    }

    #[test]
    fn sanitize_accepts_normal_paths() {
        assert_eq!(
            sanitize_relative_path("a/b\\c.txt"),
            Some(PathBuf::from("a").join("b").join("c.txt"))
        );
        assert_eq!(sanitize_relative_path("./a.txt"), Some(PathBuf::from("a.txt")));
        assert_eq!(sanitize_file_name("..."), Some("...".to_string()));
        assert_eq!(sanitize_file_name("a/b"), None);
    }

    #[cfg(unix)]
    #[test]
    fn resolve_destination_refuses_symlink_escape() {
        let base = std::env::temp_dir().join(format!("proxishare-test-{}", uuid::Uuid::new_v4()));
        let outside = std::env::temp_dir().join(format!("proxishare-out-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, base.join("link")).unwrap();

        assert!(resolve_destination(&base, Path::new("link/x.txt")).is_err());
        let ok = resolve_destination(&base, Path::new("sub/x.txt")).unwrap();
        assert!(ok.starts_with(&base));

        let _ = std::fs::remove_dir_all(&base);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
