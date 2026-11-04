//! A `FontHost` that never touches the machine.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tf2tools_core::{FontHost, FontHostError};

/// Installed names come from `names`, `fonts_dir` is `dir`; calls are recorded and each
/// operation can be made to fail.
#[derive(Default)]
pub struct FakeHost {
    pub names: Vec<String>,
    pub dir: PathBuf,
    pub fail_names: bool,
    pub fail_register: bool,
    pub fail_refresh: bool,
    pub registered: Mutex<Vec<(PathBuf, String, bool)>>,
    pub refreshes: Mutex<usize>,
}

impl FakeHost {
    pub fn new(names: &[&str], dir: &Path) -> Self {
        Self {
            names: names.iter().map(|n| (*n).to_owned()).collect(),
            dir: dir.to_path_buf(),
            ..Self::default()
        }
    }
}

impl FontHost for FakeHost {
    fn installed_names(&self) -> Result<Vec<String>, FontHostError> {
        if self.fail_names {
            return Err(FontHostError::new("fc-list is missing"));
        }
        Ok(self.names.clone())
    }

    fn fonts_dir(&self) -> Result<PathBuf, FontHostError> {
        Ok(self.dir.clone())
    }

    fn register(&self, file: &Path, full_name: &str, open_type: bool) -> Result<(), FontHostError> {
        if self.fail_register {
            return Err(FontHostError::new("registry denied"));
        }
        self.registered
            .lock()
            .unwrap()
            .push((file.to_path_buf(), full_name.to_owned(), open_type));
        Ok(())
    }

    fn refresh(&self) -> Result<(), FontHostError> {
        if self.fail_refresh {
            return Err(FontHostError::new("fc-cache failed"));
        }
        *self.refreshes.lock().unwrap() += 1;
        Ok(())
    }
}
