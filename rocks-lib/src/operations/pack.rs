use crate::build::utils;
use crate::build::utils::lua_lib_extension;
use crate::lockfile::LocalPackage;
use crate::luarocks;
use crate::luarocks::rock_manifest::RockManifest;
use crate::luarocks::rock_manifest::RockManifestBin;
use crate::luarocks::rock_manifest::RockManifestDoc;
use crate::luarocks::rock_manifest::RockManifestLib;
use crate::luarocks::rock_manifest::RockManifestLua;
use crate::luarocks::rock_manifest::RockManifestRoot;
use crate::tree::RockLayout;
use crate::tree::Tree;
use bon::{builder, Builder};
use clean_path::Clean;
use itertools::Itertools;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use tempdir::TempDir;
use thiserror::Error;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

/// A binary rock packer
#[derive(Builder)]
#[builder(start_fn = new, finish_fn(name = _build, vis = ""))]
pub struct Pack {
    #[builder(start_fn)]
    dest_dir: PathBuf,
    #[builder(start_fn)]
    tree: Tree,
    #[builder(start_fn)]
    package: LocalPackage,
}

impl<State> PackBuilder<State>
where
    State: pack_builder::State + pack_builder::IsComplete,
{
    pub fn pack(self) -> Result<PathBuf, PackError> {
        do_pack(self._build())
    }
}

#[derive(Error, Debug)]
#[error("failed to pack rock: {0}")]
pub enum PackError {
    Zip(#[from] zip::result::ZipError),
    Io(#[from] io::Error),
    Walkdir(#[from] walkdir::Error),
}

fn do_pack(args: Pack) -> Result<PathBuf, PackError> {
    let package = args.package;
    let tree = args.tree;
    let layout = tree.rock_layout(&package);
    let suffix = if is_binary_rock(&layout) {
        format!("{}.rock", luarocks::current_platform_luarocks_identifier())
    } else {
        "all.rock".into()
    };
    let file_name = format!("{}-{}.{}", package.name(), package.version(), suffix);
    let output_path = args.dest_dir.join(file_name);
    let file = File::create(&output_path)?;
    let mut zip = ZipWriter::new(file);

    let lua_entries = add_rock_entries(&mut zip, &layout.src, "lua".into())?;
    let lib_entries = add_rock_entries(&mut zip, &layout.lib, "lib".into())?;
    let doc_entries = add_rock_entries(&mut zip, &layout.doc, "doc".into())?;
    // We copy entries from `etc` to the root directory, as luarocks doesn't have an etc directory.
    let temp_dir = TempDir::new("rocks-pack-temp-root").unwrap().into_path();
    utils::recursive_copy_dir(&layout.etc, &temp_dir)?;
    // prevent duplicate doc entries
    let doc = temp_dir.join("doc");
    if doc.is_dir() {
        std::fs::remove_dir_all(&doc)?;
    }
    // luarocks expects a <package>-<version>.rockspec,
    // so we copy it the package.rockspec to our temporary root directory and rename it
    let packed_rockspec_name = format!("{}-{}.rockspec", &package.name(), &package.version());
    let renamed_rockspec_entry = temp_dir.join(packed_rockspec_name);
    std::fs::copy(layout.rockspec_path(), &renamed_rockspec_entry)?;
    let root_entries = add_rock_entries(&mut zip, &temp_dir, "".into())?;
    let mut bin_entries = HashMap::new();
    for relative_binary_path in package.spec.binaries() {
        let binary_path = tree.bin().join(
            relative_binary_path
                .clean()
                .file_name()
                .expect("malformed lockfile"),
        );
        if binary_path.is_file() {
            let (path, digest) =
                add_rock_entry(&mut zip, binary_path, &layout.bin, &PathBuf::default())?;
            bin_entries.insert(path, digest);
        }
    }
    let rock_manifest = RockManifest {
        lua: RockManifestLua {
            entries: lua_entries,
        },
        lib: RockManifestLib {
            entries: lib_entries,
        },
        doc: RockManifestDoc {
            entries: doc_entries,
        },
        root: RockManifestRoot {
            entries: root_entries,
        },
        bin: RockManifestBin {
            entries: bin_entries,
        },
    };
    let manifest_str = rock_manifest.to_lua_string();
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("rock_manifest", options)?;
    zip.write_all(manifest_str.as_bytes())?;
    Ok(output_path)
}

fn is_binary_rock(layout: &RockLayout) -> bool {
    if !&layout.lib.is_dir() {
        return false;
    }
    WalkDir::new(&layout.lib).into_iter().any(|entry| {
        entry.is_ok_and(|entry| {
            let file = entry.into_path();
            file.is_file()
                && file
                    .extension()
                    .is_some_and(|ext| ext.to_string_lossy() == lua_lib_extension())
        })
    })
}

fn add_rock_entries(
    zip: &mut ZipWriter<File>,
    source_dir: &PathBuf,
    zip_dir: PathBuf,
) -> Result<HashMap<PathBuf, String>, PackError> {
    let mut result = HashMap::new();
    if source_dir.is_dir() {
        for file in WalkDir::new(source_dir).into_iter().filter_map_ok(|entry| {
            let file = entry.into_path();
            if file.is_file() {
                Some(file)
            } else {
                None
            }
        }) {
            let file = file?;
            let (path, digest) = add_rock_entry(zip, file, source_dir, &zip_dir)?;
            result.insert(path, digest);
        }
    }
    Ok(result)
}

fn add_rock_entry(
    zip: &mut ZipWriter<File>,
    file: PathBuf,
    source_dir: &PathBuf,
    zip_dir: &Path,
) -> Result<(PathBuf, String), PackError> {
    let relative_path: PathBuf = pathdiff::diff_paths(source_dir.join(file.clone()), source_dir)
        .expect("failed get relative path!");
    let mut f = File::open(file)?;
    let mut buffer = Vec::new();
    f.read_to_end(&mut buffer)?;
    let digest = md5::compute(&buffer);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(zip_dir.join(&relative_path).to_string_lossy(), options)?;
    zip.write_all(&buffer)?;
    Ok((relative_path, format!("{:x}", digest)))
}
