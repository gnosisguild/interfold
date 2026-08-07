// SPDX-License-Identifier: LGPL-3.0-only
//
// This file is provided WITHOUT ANY WARRANTY;
// without even the implied warranty of MERCHANTABILITY
// or FITNESS FOR A PARTICULAR PURPOSE.

use std::path::{Path, PathBuf};

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
}
