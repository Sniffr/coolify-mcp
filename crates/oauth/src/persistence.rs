use crate::model::PersistedState;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub struct OAuthStateStore {
    path: PathBuf,
    state: PersistedState,
    degraded: bool,
}
impl OAuthStateStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            state: PersistedState::default(),
            degraded: false,
        }
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read(path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(state) => Ok(Self {
                    path: path.into(),
                    state,
                    degraded: false,
                }),
                Err(_) => Ok(Self {
                    path: path.into(),
                    state: PersistedState::default(),
                    degraded: true,
                }),
            },
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::new(path.into())),
            Err(e) => Err(e),
        }
    }
    pub fn state(&self) -> &PersistedState {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut PersistedState {
        &mut self.state
    }
    pub fn degraded(&self) -> bool {
        self.degraded
    }
    pub fn save(&self, state: &PersistedState) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(state).map_err(io::Error::other)?)?;
        set_private(&tmp)?;
        fs::rename(tmp, &self.path)?;
        set_private(&self.path)
    }
}
fn set_private(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
