use std::io::Read;

use cid_base::file_path::FilePath;
use cid_base::result::{CidResult, ResultExt};
use cid_pal::pal::PalHandle;

use crate::embedded_assets::EmbeddedAssets;

pub(crate) enum AssetSource {
    Embedded(EmbeddedAssets),
    Filesystem { pal: PalHandle, root: FilePath },
}

impl AssetSource {
    pub(crate) fn load(pal: PalHandle, filesystem_root: FilePath) -> CidResult<Self> {
        match EmbeddedAssets::from_current_exe()? {
            Some(embedded_assets) => Ok(Self::Embedded(embedded_assets)),
            None => Ok(Self::Filesystem {
                pal,
                root: filesystem_root,
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn filesystem(pal: PalHandle, root: FilePath) -> Self {
        Self::Filesystem { pal, root }
    }

    pub(crate) fn read(&self, relative_path: &FilePath) -> CidResult<Option<Vec<u8>>> {
        match self {
            Self::Embedded(embedded_assets) => embedded_assets.read(relative_path.as_str()),
            Self::Filesystem { pal, root } => {
                let full_path = root.join(relative_path.as_str());
                if !pal.file_exists(&full_path)? {
                    return Ok(None);
                }

                Ok(Some(read_file_bytes(pal, &full_path)?))
            }
        }
    }
}

fn read_file_bytes(pal: &PalHandle, path: &FilePath) -> CidResult<Vec<u8>> {
    let mut file = pal.read_file(path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)
        .with_context(|| format!("failed to read asset `{path}`"))?;
    Ok(buffer)
}
