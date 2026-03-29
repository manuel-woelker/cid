use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use backhand::{FilesystemReader, InnerNode};
use cid_base::result::{CidResult, OptionExt, ResultExt};

const EMBEDDED_UI_MAGIC: &[u8; 8] = b"SQUASHFS";
const EMBEDDED_UI_SIZE_LEN: u64 = 4;

pub(crate) struct EmbeddedAssets {
    filesystem: FilesystemReader<'static>,
    file_indices: HashMap<String, usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct EmbeddedSquashfsTrailer {
    pub(crate) squashfs_offset: u64,
    pub(crate) squashfs_len: u64,
}

impl EmbeddedAssets {
    pub(crate) fn from_current_exe() -> CidResult<Option<Self>> {
        let executable_path =
            std::env::current_exe().context("failed to determine current executable path")?;
        Self::from_executable_path(executable_path)
    }

    pub(crate) fn from_executable_path(
        executable_path: impl AsRef<Path>,
    ) -> CidResult<Option<Self>> {
        let executable_path = executable_path.as_ref();
        let mut executable = File::open(executable_path).with_context(|| {
            format!(
                "failed to open executable `{}` for embedded web assets",
                executable_path.display()
            )
        })?;

        let Some(trailer) = parse_embedded_squashfs_trailer(&mut executable)? else {
            return Ok(None);
        };

        let filesystem = FilesystemReader::from_reader_with_offset(
            BufReader::new(executable),
            trailer.squashfs_offset,
        )
        .with_context(|| {
            format!(
                "failed to open embedded squashfs from `{}` at offset {}",
                executable_path.display(),
                trailer.squashfs_offset
            )
        })?;

        Ok(Some(Self::new(filesystem)))
    }

    pub(crate) fn read(&self, relative_path: &str) -> CidResult<Option<Vec<u8>>> {
        let full_path = format!("/{}", relative_path.trim_start_matches('/'));
        let Some(&node_index) = self.file_indices.get(&full_path) else {
            return Ok(None);
        };

        let node = &self.filesystem.root.nodes[node_index];
        let InnerNode::File(file) = &node.inner else {
            return Ok(None);
        };

        let mut reader = self.filesystem.file(file).reader();
        let mut buffer = Vec::with_capacity(file.file_len());
        reader
            .read_to_end(&mut buffer)
            .with_context(|| format!("failed to read embedded asset `{relative_path}`"))?;
        Ok(Some(buffer))
    }

    fn new(filesystem: FilesystemReader<'static>) -> Self {
        let file_indices = filesystem
            .files()
            .enumerate()
            .filter_map(|(index, node)| match &node.inner {
                InnerNode::File(_) => Some((node.fullpath.to_string_lossy().into_owned(), index)),
                _ => None,
            })
            .collect();

        Self {
            filesystem,
            file_indices,
        }
    }
}

pub(crate) fn parse_embedded_squashfs_trailer<R>(
    reader: &mut R,
) -> CidResult<Option<EmbeddedSquashfsTrailer>>
where
    R: Read + Seek,
{
    let file_len = reader
        .seek(SeekFrom::End(0))
        .context("failed to determine executable length")?;
    if file_len < EMBEDDED_UI_MAGIC.len() as u64 {
        return Ok(None);
    }

    let mut magic = [0; EMBEDDED_UI_MAGIC.len()];
    reader
        .seek(SeekFrom::End(-(EMBEDDED_UI_MAGIC.len() as i64)))
        .context("failed to seek to embedded web-ui magic")?;
    reader
        .read_exact(&mut magic)
        .context("failed to read embedded web-ui magic")?;

    if magic != *EMBEDDED_UI_MAGIC {
        return Ok(None);
    }

    let trailer_len = EMBEDDED_UI_SIZE_LEN + EMBEDDED_UI_MAGIC.len() as u64;
    if file_len < trailer_len {
        return Err(cid_base::err!(
            "embedded web-ui trailer is truncated: executable shorter than trailer"
        ));
    }

    let mut size_bytes = [0; EMBEDDED_UI_SIZE_LEN as usize];
    reader
        .seek(SeekFrom::End(-((trailer_len) as i64)))
        .context("failed to seek to embedded web-ui size")?;
    reader
        .read_exact(&mut size_bytes)
        .context("failed to read embedded web-ui size")?;

    let squashfs_len = u32::from_le_bytes(size_bytes) as u64;
    let squashfs_offset = file_len
        .checked_sub(trailer_len + squashfs_len)
        .with_context(|| {
            format!(
                "embedded web-ui size {} exceeds executable length {}",
                squashfs_len, file_len
            )
        })?;

    Ok(Some(EmbeddedSquashfsTrailer {
        squashfs_offset,
        squashfs_len,
    }))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::time::{SystemTime, UNIX_EPOCH};

    use backhand::{FilesystemWriter, NodeHeader, compression::Compressor, kind, kind::Kind};
    use cid_base::file_path::FilePath;

    use super::{
        EMBEDDED_UI_MAGIC, EmbeddedAssets, EmbeddedSquashfsTrailer, parse_embedded_squashfs_trailer,
    };

    #[test]
    fn parse_embedded_squashfs_trailer_returns_none_without_magic() {
        let mut bytes = Cursor::new(b"plain executable".to_vec());
        assert_eq!(parse_embedded_squashfs_trailer(&mut bytes).unwrap(), None);
    }

    #[test]
    fn parse_embedded_squashfs_trailer_reads_offset_and_size() {
        let image = sample_squashfs();
        let prefix = b"binary-prefix";
        let executable = executable_with_embedded_squashfs(prefix, &image);

        let trailer = parse_embedded_squashfs_trailer(&mut Cursor::new(executable)).unwrap();
        assert_eq!(
            trailer,
            Some(EmbeddedSquashfsTrailer {
                squashfs_offset: prefix.len() as u64,
                squashfs_len: image.len() as u64,
            })
        );
    }

    #[test]
    fn parse_embedded_squashfs_trailer_rejects_out_of_bounds_size() {
        let mut executable = Vec::from(&b"tiny"[..]);
        executable.extend_from_slice(&u32::MAX.to_le_bytes());
        executable.extend_from_slice(EMBEDDED_UI_MAGIC);

        let error = parse_embedded_squashfs_trailer(&mut Cursor::new(executable)).unwrap_err();
        assert!(
            error
                .to_test_string()
                .contains("embedded web-ui size 4294967295 exceeds executable length")
        );
    }

    #[test]
    fn embedded_assets_reads_files_from_appended_squashfs() {
        let image = sample_squashfs();
        let executable = executable_with_embedded_squashfs(b"cid-binary", &image);
        let path = temp_file_path("embedded-web-ui");
        std::fs::write(&path, executable).unwrap();

        let embedded_assets = EmbeddedAssets::from_executable_path(&path)
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8(embedded_assets.read("index.html").unwrap().unwrap()).unwrap(),
            "<!doctype html><title>cid</title>"
        );
        assert_eq!(
            String::from_utf8(embedded_assets.read("assets/app.js").unwrap().unwrap()).unwrap(),
            "console.log('cid');"
        );
    }

    fn sample_squashfs() -> Vec<u8> {
        let mut filesystem = FilesystemWriter::default();
        filesystem.set_no_padding();
        filesystem
            .set_compressor(backhand::FilesystemCompressor::new(Compressor::Zstd, None).unwrap());
        filesystem.set_only_root_id();
        filesystem.set_kind(Kind::from_const(kind::LE_V4_0).unwrap());
        filesystem.set_root_mode(0o755);

        let header = NodeHeader {
            permissions: 0o644,
            ..NodeHeader::default()
        };

        filesystem
            .push_dir_all(
                "assets",
                NodeHeader {
                    permissions: 0o755,
                    ..header
                },
            )
            .unwrap();
        filesystem
            .push_file(
                Cursor::new(b"<!doctype html><title>cid</title>".to_vec()),
                "index.html",
                header,
            )
            .unwrap();
        filesystem
            .push_file(
                Cursor::new(b"console.log('cid');".to_vec()),
                "assets/app.js",
                header,
            )
            .unwrap();

        let mut image = Vec::new();
        filesystem.write(&mut Cursor::new(&mut image)).unwrap();
        image
    }

    fn executable_with_embedded_squashfs(prefix: &[u8], image: &[u8]) -> Vec<u8> {
        let mut executable = prefix.to_vec();
        executable.extend_from_slice(image);
        executable.extend_from_slice(&(image.len() as u32).to_le_bytes());
        executable.extend_from_slice(EMBEDDED_UI_MAGIC);
        executable
    }

    fn temp_file_path(prefix: &str) -> FilePath {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        FilePath::new(std::env::temp_dir().join(format!("cid-{prefix}-{unique}")))
    }
}
