use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("destination exists; pass overwrite to replace it: {0}")]
    Exists(PathBuf),
    #[error("output cannot replace the source video")]
    SourceCollision,
    #[error("{0}")]
    Io(#[from] io::Error),
}

/// Writes `data` beside `dest` and moves it into place in one step.
///
/// Without `overwrite`, an existing destination is never replaced (hard link fails with
/// EEXIST), so there is no check-then-write race.
pub fn atomic_write(dest: &Path, data: &[u8], overwrite: bool) -> Result<(), WriteError> {
    let dir = dest.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let mut tmp = tempfile_in(dir)?;
    tmp.1.write_all(data)?;
    tmp.1.sync_all()?;
    drop(tmp.1);
    let result = if overwrite {
        fs::rename(&tmp.0, dest).map_err(WriteError::from)
    } else {
        match fs::hard_link(&tmp.0, dest) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => Err(WriteError::Exists(dest.to_path_buf())),
            Err(e) => Err(e.into()),
        }
    };
    let _ = fs::remove_file(&tmp.0);
    result
}

/// Rejects writing analysis output over the source footage.
pub fn check_not_source(dest: &Path, source: &Path) -> Result<(), WriteError> {
    let canon = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if canon(dest) == canon(source) {
        return Err(WriteError::SourceCollision);
    }
    Ok(())
}

fn tempfile_in(dir: &Path) -> io::Result<(PathBuf, fs::File)> {
    for attempt in 0..16u32 {
        let name = format!(".tracker-{}-{}-{attempt}.tmp", std::process::id(), uuid::Uuid::new_v4().simple());
        let path = dir.join(name);
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => return Ok((path, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("could not create a temporary file"))
}

/// A cheap identity check for source footage: byte length plus a hash of the first MiB.
/// Stronger than comparing dimensions; not a cryptographic proof of identity.
pub fn fingerprint(path: &Path) -> io::Result<String> {
    let mut f = fs::File::open(path)?;
    let len = f.metadata()?.len();
    let mut buf = vec![0u8; 1 << 20];
    let mut filled = 0;
    while filled < buf.len() {
        let n = f.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    let mut h: u64 = 0xcbf29ce484222325;
    for b in &buf[..filled] {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    Ok(format!("{len}:{h:016x}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_clobber_and_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("out.txt");
        atomic_write(&p, b"one", false).unwrap();
        assert!(matches!(atomic_write(&p, b"two", false), Err(WriteError::Exists(_))));
        assert_eq!(fs::read(&p).unwrap(), b"one");
        atomic_write(&p, b"two", true).unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"two");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1, "temporary files cleaned up");
        assert!(matches!(check_not_source(&p, &p), Err(WriteError::SourceCollision)));
    }
}
