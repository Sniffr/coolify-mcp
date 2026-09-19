use std::{
    collections::HashMap,
    fmt, fs,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TokenSourceError {
    #[error("token is missing (set COOLIFY_ACCESS_TOKEN or COOLIFY_TOKEN)")]
    Missing,
    #[error("could not read token file")]
    Read(#[source] std::io::Error),
    #[error("token file is empty")]
    Empty,
}

#[derive(Clone)]
pub struct TokenSource {
    inner: Arc<RwLock<TokenInner>>,
}

enum TokenInner {
    Inline(String),
    File(PathBuf),
}

impl TokenSource {
    pub fn from_env(env: &HashMap<String, String>) -> Result<Self, TokenSourceError> {
        if let Some(path) = env
            .get("COOLIFY_ACCESS_TOKEN_FILE")
            .filter(|v| !v.trim().is_empty())
        {
            return Self::from_file(path);
        }
        let token = env
            .get("COOLIFY_ACCESS_TOKEN")
            .filter(|v| !v.is_empty())
            .or_else(|| env.get("COOLIFY_TOKEN"))
            .ok_or(TokenSourceError::Missing)?;
        if token.is_empty() {
            return Err(TokenSourceError::Empty);
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(TokenInner::Inline(token.clone()))),
        })
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, TokenSourceError> {
        let path = path.as_ref().to_path_buf();
        let value = fs::read_to_string(&path).map_err(TokenSourceError::Read)?;
        if value.trim().is_empty() {
            return Err(TokenSourceError::Empty);
        }
        Ok(Self {
            inner: Arc::new(RwLock::new(TokenInner::File(path))),
        })
    }

    pub fn current(&self) -> Result<String, TokenSourceError> {
        let inner = self.inner.read().expect("token lock poisoned");
        match &*inner {
            TokenInner::Inline(value) => Ok(value.clone()),
            TokenInner::File(path) => {
                let value = fs::read_to_string(path).map_err(TokenSourceError::Read)?;
                let value = value.trim_end().to_owned();
                if value.is_empty() {
                    Err(TokenSourceError::Empty)
                } else {
                    Ok(value)
                }
            }
        }
    }

    pub fn refresh(&self) -> Result<(), TokenSourceError> {
        self.current().map(|_| ())
    }
}

impl fmt::Debug for TokenSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSource")
            .field("configured", &true)
            .finish()
    }
}
