//! §6 the path-copy primitives — the git-free heart [`super::install`] dispatches
//! to by path shape: a folder MIRRORS (rsync `--delete`, deletions propagate), a
//! file/glob UNIONS (additive, source-wins). The gitignored `bin/` subtree is
//! never walked. Lifted from [`super`] so the install file stays the shape
//! dispatch + the plugin-binding half; pure dir IO, unit-tested on tempfiles.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::Summary;

/// Make `dst` byte-identical to `src` (rsync `--delete`): remove every `dst` file
/// the `src` lacks, then copy every `src` file in. The gitignored `bin/` subtree
/// under either root is never walked, so it is neither copied nor deleted.
pub(super) fn mirror(src: &Path, dst: &Path, src_bin: &Path, dst_bin: &Path) -> io::Result<Summary> {
    let mut deleted = 0;
    for file in walk(dst, dst_bin)? {
        let rel = file.strip_prefix(dst).expect("walk yields dst-rooted paths");
        if !src.join(rel).is_file() {
            fs::remove_file(&file)?;
            deleted += 1;
        }
    }
    let mut added = 0;
    for file in walk(src, src_bin)? {
        let rel = file.strip_prefix(src).expect("walk yields src-rooted paths");
        copy_file(&file, &dst.join(rel))?;
        added += 1;
    }
    Ok(Summary { added, deleted })
}

/// Union one source file into the destination, overwriting on overlap. An absent
/// source file copies nothing — there is nothing to port.
pub(super) fn union_file(src: &Path, dst: &Path) -> io::Result<Summary> {
    if src.is_file() {
        copy_file(src, dst)?;
        return Ok(Summary { added: 1, deleted: 0 });
    }
    Ok(Summary::default())
}

/// Union every source file in `<dir>` whose name matches the trailing `*`-glob
/// into the destination's `<dir>`, source-wins on overlap. Directories (incl.
/// `bin/`) never match — a glob copies files, additively.
pub(super) fn union_glob(path: &str, from: &Path, to: &Path) -> io::Result<Summary> {
    let (dir, pattern) = path.rsplit_once('/').unwrap_or(("", path));
    let src_dir = from.join(dir);
    let dst_dir = to.join(dir);
    let mut added = 0;
    if src_dir.is_dir() {
        for entry in fs::read_dir(&src_dir)? {
            let entry = entry?;
            let name = entry.file_name();
            if entry.file_type()?.is_file() && matches(pattern, &name.to_string_lossy()) {
                copy_file(&entry.path(), &dst_dir.join(&name))?;
                added += 1;
            }
        }
    }
    Ok(Summary { added, deleted: 0 })
}

/// Every regular file under `root` (recursive), skipping the `skip` subtree.
fn walk(root: &Path, skip: &Path) -> io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if root.is_dir() {
        walk_into(root, skip, &mut out)?;
    }
    Ok(out)
}

fn walk_into(dir: &Path, skip: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path == *skip {
            continue;
        }
        if entry.file_type()?.is_dir() {
            walk_into(&path, skip, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Copy `src` to `dst`, making parents — the one write primitive both shapes use.
fn copy_file(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst.parent().expect("a copy target always has a parent"))?;
    fs::copy(src, dst)?;
    Ok(())
}

/// Minimal `*`-glob: `*` matches any (possibly empty) run of characters; every
/// other character is literal (no `?`/`[]`). Multiple `*` compose.
pub(super) fn matches(pattern: &str, name: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == name,
        Some((prefix, rest)) => {
            let Some(after) = name.strip_prefix(prefix) else {
                return false;
            };
            (0..=after.len()).any(|i| after.is_char_boundary(i) && matches(rest, &after[i..]))
        }
    }
}
