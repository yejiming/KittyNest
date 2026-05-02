use crate::models::ObsidianVault;
use std::path::{Path, PathBuf};

/// Scan common locations for Obsidian vaults (directories containing .obsidian/).
pub fn detect_vaults() -> Vec<ObsidianVault> {
    let mut vaults = Vec::new();
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return vaults,
    };

    let search_roots: Vec<PathBuf> = vec![
        home.join("Library").join("Mobile Documents"),
        home.join("Documents"),
        home.join("Desktop"),
        home.clone(),
    ];

    for root in &search_roots {
        if root.exists() {
            scan_dir_for_vaults(root, &mut vaults, 0, 3);
        }
    }

    // Deduplicate by canonical path
    vaults.sort_by(|a, b| a.path.cmp(&b.path));
    vaults.dedup_by(|a, b| a.path == b.path);

    vaults
}

fn scan_dir_for_vaults(
    dir: &Path,
    vaults: &mut Vec<ObsidianVault>,
    depth: usize,
    max_depth: usize,
) {
    if depth > max_depth {
        return;
    }

    // Check if this directory is a vault
    if dir.join(".obsidian").exists() {
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Vault".to_string());
        vaults.push(ObsidianVault {
            path: dir.to_string_lossy().to_string(),
            name,
        });
        return; // Don't recurse into vaults
    }

    // Recurse into subdirectories
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories and common non-vault dirs
            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
            if dir_name.starts_with('.') || dir_name == "node_modules" || dir_name == "target" {
                continue;
            }
            scan_dir_for_vaults(&path, vaults, depth + 1, max_depth);
        }
    }
}

/// Validate that a path is a valid Obsidian vault.
pub fn validate_vault(path: &str) -> bool {
    let p = Path::new(path);
    p.exists() && p.join(".obsidian").exists()
}
