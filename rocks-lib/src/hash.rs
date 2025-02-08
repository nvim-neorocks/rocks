use bytes::Bytes;
use git2::Repository;
use itertools::Itertools;
use ssri::{Algorithm, Integrity, IntegrityOpts};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tempdir::TempDir;
use walkdir::WalkDir;

pub trait HasIntegrity {
    fn hash(&self) -> io::Result<Integrity>;
}

impl HasIntegrity for PathBuf {
    fn hash(&self) -> io::Result<Integrity> {
        let mut integrity_opts = IntegrityOpts::new().algorithm(Algorithm::Sha256);
        if self.is_dir() {
            for relative_path in WalkDir::new(self)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| !e.file_type().is_dir())
                .map(|e| e.path().strip_prefix(self).unwrap().to_path_buf())
                .sorted()
            {
                hash_file(&self.join(relative_path), &mut integrity_opts)?;
            }
        } else if self.is_file() {
            hash_file(self, &mut integrity_opts)?;
        }
        Ok(integrity_opts.result())
    }
}

impl HasIntegrity for Path {
    fn hash(&self) -> io::Result<Integrity> {
        let path_buf: PathBuf = self.into();
        path_buf.hash()
    }
}

impl HasIntegrity for TempDir {
    fn hash(&self) -> io::Result<Integrity> {
        self.path().hash()
    }
}

impl HasIntegrity for Bytes {
    fn hash(&self) -> io::Result<Integrity> {
        let mut integrity_opts = IntegrityOpts::new().algorithm(Algorithm::Sha256);
        integrity_opts.input(self);
        Ok(integrity_opts.result())
    }
}

impl HasIntegrity for Repository {
    fn hash(&self) -> io::Result<Integrity> {
        let mut integrity_opts = IntegrityOpts::new().algorithm(Algorithm::Sha256);

        let odb = self
            .odb()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        let mut objects = Vec::new();
        odb.foreach(|oid| {
            if let Ok(obj) = odb.read(*oid) {
                objects.push((*oid, obj.data().to_vec()));
            }
            true
        })
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        // Sort by OID to ensure deterministic order
        objects.sort_by_key(|(oid, _)| *oid);

        for (_, data) in objects {
            integrity_opts.input(&data);
        }

        Ok(integrity_opts.result())
    }
}

fn hash_file(path: &Path, integrity_opts: &mut IntegrityOpts) -> io::Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    integrity_opts.input(&buffer);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_fs::prelude::*;
    use git2::{Index, Signature, Time};
    use std::fs::write;

    #[test]
    fn test_hash_empty_dir() {
        let temp = assert_fs::TempDir::new().unwrap();
        let hash1 = temp.path().to_path_buf().hash().unwrap();
        let hash2 = temp.path().to_path_buf().hash().unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_single_file() {
        let temp = assert_fs::TempDir::new().unwrap();
        let file = temp.child("test.txt");
        file.write_str("test content").unwrap();

        let hash1 = temp.path().to_path_buf().hash().unwrap();
        let hash2 = temp.path().to_path_buf().hash().unwrap();
        assert_eq!(hash1, hash2)
    }

    #[test]
    fn test_hash_multiple_files_different_creation_order() {
        let temp = assert_fs::TempDir::new().unwrap();

        write(temp.child("a.txt").path(), "content a").unwrap();
        write(temp.child("b.txt").path(), "content b").unwrap();
        write(temp.child("c.txt").path(), "content c").unwrap();
        let hash1 = temp.path().to_path_buf().hash().unwrap();

        let temp2 = assert_fs::TempDir::new().unwrap();
        write(temp2.child("c.txt").path(), "content c").unwrap();
        write(temp2.child("a.txt").path(), "content a").unwrap();
        write(temp2.child("b.txt").path(), "content b").unwrap();
        let hash2 = temp2.path().to_path_buf().hash().unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_nested_directories_different_creation_order() {
        let temp = assert_fs::TempDir::new().unwrap();

        temp.child("a/b").create_dir_all().unwrap();
        temp.child("b").create_dir_all().unwrap();
        write(temp.child("a/b/file1.txt").path(), "content 1").unwrap();
        write(temp.child("a/file2.txt").path(), "content 2").unwrap();
        write(temp.child("b/file3.txt").path(), "content 3").unwrap();
        let hash1 = temp.path().to_path_buf().hash().unwrap();

        let temp2 = assert_fs::TempDir::new().unwrap();
        temp2.child("a/b").create_dir_all().unwrap();
        temp2.child("b").create_dir_all().unwrap();
        write(temp2.child("b/file3.txt").path(), "content 3").unwrap();
        write(temp2.child("a/file2.txt").path(), "content 2").unwrap();
        write(temp2.child("a/b/file1.txt").path(), "content 1").unwrap();
        let hash2 = temp2.path().to_path_buf().hash().unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_with_empty_directories() {
        let temp = assert_fs::TempDir::new().unwrap();

        temp.child("empty_dir").create_dir_all().unwrap();
        write(temp.child("file.txt").path(), "content").unwrap();
        let hash1 = temp.path().to_path_buf().hash().unwrap();

        let temp2 = assert_fs::TempDir::new().unwrap();
        write(temp2.child("file.txt").path(), "content").unwrap();
        let hash2 = temp2.path().to_path_buf().hash().unwrap();

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_with_different_line_endings() {
        let temp = assert_fs::TempDir::new().unwrap();
        write(temp.child("unix.txt").path(), "line1\nline2\n").unwrap();
        let hash1 = temp.path().to_path_buf().hash().unwrap();

        let temp2 = assert_fs::TempDir::new().unwrap();
        write(temp2.child("windows.txt").path(), "line1\r\nline2\r\n").unwrap();
        let hash2 = temp2.path().to_path_buf().hash().unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_with_symlinks() {
        let temp = assert_fs::TempDir::new().unwrap();

        write(temp.child("target.txt").path(), "content").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(
            temp.child("target.txt").path(),
            temp.child("link.txt").path(),
        )
        .unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_file(
            temp.child("target.txt").path(),
            temp.child("link.txt").path(),
        )
        .unwrap();

        let hash1 = temp.path().to_path_buf().hash().unwrap();

        let temp2 = assert_fs::TempDir::new().unwrap();
        write(temp2.child("target.txt").path(), "content").unwrap();
        let hash2 = temp2.path().to_path_buf().hash().unwrap();

        assert_ne!(hash1, hash2);
    }

    fn make_signature() -> Result<Signature<'static>, git2::Error> {
        let time = Time::new(1234567890, 0);
        Signature::new("Test User", "test@example.com", &time)
    }

    fn create_commit(
        repo: &Repository,
        tree: &git2::Tree,
        message: &str,
        parents: &[&git2::Commit],
    ) -> Result<git2::Oid, git2::Error> {
        let sig = make_signature()?;
        repo.commit(Some("HEAD"), &sig, &sig, message, tree, parents)
    }

    fn add_file_to_index(
        repo: &Repository,
        index: &mut Index,
        name: &str,
        content: &[u8],
    ) -> Result<(), git2::Error> {
        let path = repo.workdir().unwrap().join(name);
        std::fs::write(&path, content).unwrap();
        index.add_path(Path::new(name))?;
        index.write()?;
        Ok(())
    }

    #[test]
    fn test_empty_repo_hash() {
        let repo_dir = assert_fs::TempDir::new().unwrap();
        let repo = Repository::init(repo_dir.path()).unwrap();
        repo.hash().unwrap();
    }

    #[test]
    fn test_hash_is_reproducible() {
        let repo_dir = assert_fs::TempDir::new().unwrap();
        let repo = Repository::init(repo_dir.path()).unwrap();
        let mut index = repo.index().unwrap();

        add_file_to_index(&repo, &mut index, "test.txt", b"test content").unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        create_commit(&repo, &tree, "test commit", &[]).unwrap();

        let hash1 = repo.hash().unwrap();
        let hash2 = repo.hash().unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_repository_hash_differs_with_different_content() {
        let repo_dir = assert_fs::TempDir::new().unwrap();
        let repo = Repository::init(repo_dir.path()).unwrap();

        let test_content = b"test content";
        let file_path = repo_dir.path().join("test.txt");
        std::fs::write(&file_path, test_content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();

        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        let sig = make_signature().unwrap();

        let initial_commit = repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();

        let hash1 = repo.hash().unwrap();

        let different_content = b"different content";
        std::fs::write(&file_path, different_content).unwrap();

        index.add_path(Path::new("test.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent_commit = repo.find_commit(initial_commit).unwrap();
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "second commit",
            &tree,
            &[&parent_commit],
        )
        .unwrap();

        let hash2 = repo.hash().unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_repository_hash_with_all_object_types_reproducible() {
        let repo_dir = assert_fs::TempDir::new().unwrap();
        let repo = Repository::init(repo_dir.path()).unwrap();
        let mut index = repo.index().unwrap();
        add_file_to_index(&repo, &mut index, "test.txt", b"test content").unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let commit_id = create_commit(&repo, &tree, "test commit", &[]).unwrap();
        let sig = make_signature().unwrap();
        let commit = repo.find_commit(commit_id).unwrap();
        repo.tag("v1.0.0", commit.as_object(), &sig, "test tag", false)
            .unwrap();
        let hash1 = repo.hash().unwrap();
        let hash2 = repo.hash().unwrap();
        assert_eq!(hash1, hash2);
    }
}
