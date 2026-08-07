// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::path::{Path, PathBuf};

/// Permission mode for the directories that the support container writes to.
///
/// The capital `X` adds the execute bit to directories only. Plain files do not
/// get the execute bit. A directory needs it to stay traversable. Solidity and
/// TypeScript sources do not need it.
pub const CONTAINER_WRITABLE_MODE: &str = "a+rwX";

/// Lists the directories that the support container mounts read-write.
///
/// The RISC Zero build script in `crates/support/methods/build.rs` writes
/// `ImageID.sol` to `../contracts` and `Elf.sol` to `../tests`, both relative
/// to `/app`. `ctl/container` maps these two paths to
/// `.interfold/generated/contracts` and `tests` on the host.
pub fn container_writable_paths(cwd: &Path) -> Vec<PathBuf> {
    vec![
        cwd.join(".interfold").join("generated").join("contracts"),
        cwd.join("tests"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn writable_paths_are_the_directories_the_container_mounts() {
        let cwd = Path::new("/proj");
        let paths = container_writable_paths(cwd);

        assert!(paths.contains(&cwd.join(".interfold/generated/contracts")));
        assert!(paths.contains(&cwd.join("tests")));
    }

    #[test]
    fn the_projects_own_contracts_folder_is_not_widened() {
        let cwd = Path::new("/proj");
        let paths = container_writable_paths(cwd);

        // `contracts/` holds the project's own Solidity sources. Neither
        // `ctl/container` nor `support/scripts/dev.sh` mounts it into the
        // container. Wider permissions there give access that nothing needs.
        assert!(!paths.contains(&cwd.join("contracts")));
    }

    #[tokio::test]
    async fn writable_mode_keeps_directories_traversable() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let nested = root.path().join("nested");
        tokio::fs::create_dir(&nested).await?;

        crate::file_utils::chmod_recursive(root.path(), CONTAINER_WRITABLE_MODE).await?;

        // Without the execute bit, no process can enter a directory. The
        // container cannot then reach the files inside it.
        assert_eq!(mode_of(&nested).await? & 0o111, 0o111);
        assert_eq!(mode_of(&nested).await? & 0o222, 0o222);
        Ok(())
    }

    #[tokio::test]
    async fn writable_mode_does_not_mark_sources_executable() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let source = root.path().join("MyProgram.sol");
        tokio::fs::write(&source, "// contract").await?;

        crate::file_utils::chmod_recursive(root.path(), CONTAINER_WRITABLE_MODE).await?;

        // Solidity sources are not programs. `777` set the execute bit on
        // every source file. The template still carries `100755` blobs from
        // an earlier run.
        assert_eq!(mode_of(&source).await? & 0o111, 0);
        assert_eq!(mode_of(&source).await? & 0o222, 0o222);
        Ok(())
    }

    async fn mode_of(path: &Path) -> anyhow::Result<u32> {
        use std::os::unix::fs::PermissionsExt;
        Ok(tokio::fs::metadata(path).await?.permissions().mode() & 0o777)
    }
}
