use super::*;

impl NativeResourceLoader {
    pub(crate) fn load_local_blob_media(
        &mut self,
        document_url: &str,
        src: &str,
        object_url: Option<&NativeObjectUrlResource>,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("media URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "media owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_subresource_url_with_blob(&document_url, src)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("blob") {
            return Ok(None);
        }
        if is_network_url(document_url.as_str()) {
            if !mixed_content_allowed(&document_url, &target_url) {
                return Ok(None);
            }
            let policy = self
                .network
                .document_policies
                .get(&cache_key(&document_url))
                .cloned()
                .unwrap_or_default();
            self.record_report_only_url_violations(
                &policy,
                NativeSubresourceKind::Media,
                &document_url,
                &target_url,
            );
            if !policy.allows(NativeSubresourceKind::Media, &document_url, &target_url) {
                return Ok(None);
            }
        }
        let Some(object_url) = object_url else {
            return Ok(None);
        };
        NativeOrigin::from_blob_url(without_fragment(target_url.as_str()))?;
        media_metadata_from_bytes(object_url.content_type.as_deref(), &object_url.body)
    }

    /// Admit a file-backed media resource for an explicitly rooted file
    /// document. The file is canonicalized and bounded before MIME metadata
    /// is exposed; no file bytes enter the network or HTTP cache owners.
    pub(crate) fn load_local_file_media(
        &self,
        document_url: &str,
        src: &str,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("media URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "media owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        if !is_file_url(document_url.as_str()) {
            return Ok(None);
        }
        let Some(target_url) = resolve_media_url(&document_url, src)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("file") {
            return Ok(None);
        }
        let path = match self.allowed_file_path(&target_url, "file media") {
            Ok(path) => path,
            Err(NativeEngineError::UnsupportedUrl { .. }) => return Ok(None),
            Err(error) => return Err(error),
        };
        if !self.rooted_file_subresource_allowed(
            document_url.as_str(),
            &target_url,
            &path,
            NativeSubresourceKind::Media,
            "file media CSP owner URL",
        )? {
            return Ok(None);
        }
        let bytes = read_bounded_file(&path, MAX_NATIVE_MEDIA_BYTES, "file media")?;
        media_metadata_from_bytes(file_media_type(&path), &bytes)
    }

    pub(super) fn load_file_media_url(
        &self,
        target_url: &Url,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        let path = self.allowed_file_path(target_url, "file media")?;
        let bytes = read_bounded_file(&path, MAX_NATIVE_MEDIA_BYTES, "file media")?;
        media_metadata_from_bytes(file_media_type(&path), &bytes)
    }

    /// Load a data URL media resource without entering the network owner.
    /// Embedded bytes remain subject to the same bounded media metadata
    /// admission as Blob and HTTP(S) resources.
    pub(crate) fn load_data_media(
        &self,
        document_url: &str,
        src: &str,
    ) -> Result<Option<NativeMediaMetadata>, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("media URL", src)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "media owner URL is not valid URL syntax".into(),
            }
        })?;
        reject_credentials(&document_url)?;
        let Some(target_url) = resolve_media_url(&document_url, src)? else {
            return Ok(None);
        };
        if !target_url.scheme().eq_ignore_ascii_case("data") {
            return Ok(None);
        }
        data_media_metadata(&target_url)
    }
}
