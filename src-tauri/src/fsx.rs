//! File helpers.

use std::io::Write;
use std::path::Path;

/// Writes through a temporary file in the same folder, then renames it over the target, so a crash
/// never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let tmp = dir.join(format!(".{}.lawha-tmp", path.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}
