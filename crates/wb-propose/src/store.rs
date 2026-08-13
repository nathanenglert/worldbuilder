//! Reading and writing the `proposals/` folder.
//!
//! Decided proposals stay on disk with their status recorded rather than being deleted.
//! They are the log of what was asked for and what the writer decided — provenance that
//! matters more once agents start filling the queue.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::{Proposal, Status};

pub const DIR: &str = "proposals";

/// Every proposal in the world, oldest filename first.
pub fn load_all(root: impl AsRef<Path>) -> Result<Vec<Proposal>> {
    let dir = root.as_ref().join(DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|source| Error::Io { path: dir.clone(), source })?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().and_then(|e| e.to_str()).is_some_and(|e| e == "yaml" || e == "yml")
        })
        .filter(|p| !p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')))
        .collect();
    paths.sort();

    paths.iter().map(|path| read(path)).collect()
}

pub fn read(path: &Path) -> Result<Proposal> {
    let text = fs::read_to_string(path)
        .map_err(|source| Error::Io { path: path.to_path_buf(), source })?;
    let mut proposal: Proposal = serde_yaml_bw::from_str(&text)
        .map_err(|e| Error::Yaml { path: path.to_path_buf(), message: e.to_string() })?;
    proposal.source = path.to_path_buf();
    Ok(proposal)
}

/// Write a proposal, creating `proposals/` if needed. Uses its existing file when it
/// has one, so recording a decision never orphans the original.
pub fn write(root: impl AsRef<Path>, proposal: &Proposal) -> Result<PathBuf> {
    let path = if proposal.source.as_os_str().is_empty() {
        let dir = root.as_ref().join(DIR);
        fs::create_dir_all(&dir).map_err(|source| Error::Io { path: dir.clone(), source })?;
        dir.join(format!("{}.yaml", proposal.id))
    } else {
        proposal.source.clone()
    };

    let yaml = serde_yaml_bw::to_string(proposal)
        .map_err(|e| Error::Yaml { path: path.clone(), message: e.to_string() })?;
    fs::write(&path, yaml).map_err(|source| Error::Io { path: path.clone(), source })?;
    Ok(path)
}

/// Record a decision on a proposal already on disk.
pub fn set_status(proposal: &mut Proposal, status: Status) -> Result<PathBuf> {
    if !proposal.is_pending() {
        return Err(Error::NotPending {
            proposal: proposal.id.clone(),
            status: proposal.status.slug(),
        });
    }
    proposal.status = status;
    let root = proposal
        .source
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    write(root, proposal)
}
