use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Paths {
    pub state_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub db_path: PathBuf,
    pub art_dir: PathBuf,
    pub socket_path: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Self> {
        let base_dirs = BaseDirs::new().context("could not determine the home directory")?;
        let state = env::var_os("XDG_STATE_HOME").map(PathBuf::from);
        let cache = env::var_os("XDG_CACHE_HOME").map(PathBuf::from);
        let runtime = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);

        Ok(Self::from_env_with(
            base_dirs.home_dir(),
            state.as_deref(),
            cache.as_deref(),
            runtime.as_deref(),
        ))
    }

    pub fn from_env_with(
        home: &Path,
        state: Option<&Path>,
        cache: Option<&Path>,
        runtime: Option<&Path>,
    ) -> Self {
        let state_root = state
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".local/state"));
        let cache_root = cache
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".cache"));
        let runtime_root = runtime
            .map(Path::to_path_buf)
            .unwrap_or_else(|| home.join(".local/state"));

        let state_dir = state_root.join("finalboss");
        let cache_dir = cache_root.join("finalboss");
        let runtime_dir = runtime_root.join("finalboss");

        Self {
            db_path: state_dir.join("library.sqlite"),
            art_dir: cache_dir.join("art"),
            socket_path: runtime_dir.join("backend.sock"),
            state_dir,
            cache_dir,
            runtime_dir,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        for directory in [
            &self.state_dir,
            &self.cache_dir,
            &self.runtime_dir,
            &self.art_dir,
        ] {
            fs::create_dir_all(directory)
                .with_context(|| format!("could not create {}", directory.display()))?;
        }
        Ok(())
    }
}
