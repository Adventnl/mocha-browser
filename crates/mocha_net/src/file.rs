//! Local file loading (`file://` URLs and bare paths).

use mocha_error::{MochaError, MochaResult};
use mocha_url::Url;

use crate::content_type::content_type_for_path;
use crate::ResourceResponse;

/// Load a local file referenced by `url` (scheme must be `file`).
pub(crate) fn load_file(url: &Url) -> MochaResult<ResourceResponse> {
    let path = &url.path;
    let metadata = std::fs::metadata(path)
        .map_err(|error| MochaError::Io(format!("cannot read {path}: {error}")))?;
    if metadata.is_dir() {
        return Err(MochaError::UnsupportedFeature(format!(
            "cannot load a directory as a document: {path}"
        )));
    }
    let body = std::fs::read(path)
        .map_err(|error| MochaError::Io(format!("cannot read {path}: {error}")))?;

    Ok(ResourceResponse {
        final_url: url.clone(),
        status: None,
        headers: Vec::new(),
        content_type: Some(content_type_for_path(path).to_string()),
        body,
        from_cache: false,
    })
}
