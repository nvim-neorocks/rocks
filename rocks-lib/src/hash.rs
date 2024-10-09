use eyre::Result;
use ssri::{Algorithm, Integrity, IntegrityOpts};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use tempdir::TempDir;
use walkdir::WalkDir;

pub trait HasIntegrity {
    fn hash(&self) -> Result<Integrity>;
}

impl HasIntegrity for PathBuf {
    fn hash(&self) -> Result<Integrity> {
        let mut integrity_opts = IntegrityOpts::new().algorithm(Algorithm::Sha256);
        if self.is_dir() {
            for entry in WalkDir::new(self) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    hash_file(entry.path(), &mut integrity_opts)?;
                }
            }
        } else if self.is_file() {
            hash_file(self, &mut integrity_opts)?;
        }
        Ok(integrity_opts.result())
    }
}

impl HasIntegrity for Path {
    fn hash(&self) -> Result<Integrity> {
        let path_buf: PathBuf = self.into();
        path_buf.hash()
    }
}

impl HasIntegrity for TempDir {
    fn hash(&self) -> Result<Integrity> {
        self.path().hash()
    }
}

fn hash_file(path: &Path, integrity_opts: &mut IntegrityOpts) -> Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    integrity_opts.input(&buffer);
    Ok(())
}
