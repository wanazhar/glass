use super::*;

impl NativeResourceLoader {
    pub(crate) fn cookie_profile(&self) -> Vec<NativeCookieProfileEntry> {
        self.network.cookie_profile()
    }

    pub(crate) fn shared_cookie_jar(&self) -> NativeCookieJar {
        self.network.cookies.shared_clone()
    }

    pub fn use_shared_cookie_jar(&mut self, cookie_jar: NativeCookieJar) {
        self.network.cookies = cookie_jar;
    }

    pub(crate) fn cookie_change_checkpoint(&self) -> usize {
        self.cookie_changes.len()
    }

    pub(crate) fn cookie_changes_since(&self, checkpoint: usize) -> Vec<NativeCookieChange> {
        self.cookie_changes
            .get(checkpoint..)
            .unwrap_or_default()
            .to_vec()
    }

    /// Commit an isolated loader transaction while retaining this browser
    /// context's shared cookie authority and applying only the transaction's
    /// cookie delta. Replacing the complete snapshot could erase sibling
    /// frame/target updates made while the transaction awaited content IPC.
    pub(crate) fn commit_staged_cookie_state(
        &mut self,
        mut staged: Self,
        cookie_changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
        let cookie_jar = self.network.cookies.shared_clone();
        cookie_jar.apply_changes(cookie_changes)?;
        staged.network.cookies = cookie_jar;
        *self = staged;
        Ok(())
    }

    pub(crate) fn cookies_for_document(
        &self,
        document_url: &str,
    ) -> Result<Vec<NativeCookieProfileEntry>, NativeEngineError> {
        validate_url_text("cookie owner URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "cookie owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(Vec::new());
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .matching_cookies(&document_url, true)
            .into_iter()
            .filter_map(|cookie| cookie.to_profile())
            .collect())
    }

    pub(crate) fn set_cookie_profiles(
        &mut self,
        profiles: &[NativeCookieProfileEntry],
    ) -> Result<(), NativeEngineError> {
        if !self.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "install content-process cookies".into(),
                reason: "cookie profiles are owned by the browser process".into(),
            });
        }
        if profiles.len() > MAX_NATIVE_COOKIE_PROFILE_ENTRIES {
            return Err(NativeEngineError::limit(
                "native cookie profile entries",
                MAX_NATIVE_COOKIE_PROFILE_ENTRIES,
                profiles.len(),
            ));
        }
        for profile in profiles {
            let Some(cookie) = NativeCookie::from_profile(profile.clone())? else {
                if self
                    .network
                    .remove_cookie(&profile.name, &profile.domain, &profile.path)
                {
                    self.cookie_changes.push(NativeCookieChange {
                        name: profile.name.clone(),
                        domain: profile.domain.clone(),
                        path: profile.path.clone(),
                        cookie: None,
                    });
                }
                continue;
            };
            if let Some(evicted) = self.network.set_cookie_profile(cookie)? {
                self.cookie_changes.push(NativeCookieChange {
                    name: evicted.name,
                    domain: evicted.domain,
                    path: evicted.path,
                    cookie: None,
                });
            }
            self.cookie_changes.push(NativeCookieChange {
                name: profile.name.clone(),
                domain: profile.domain.clone(),
                path: profile.path.clone(),
                cookie: Some(profile.clone()),
            });
        }
        Ok(())
    }

    pub(crate) fn clear_cookies(&mut self) {
        self.network.clear_cookies(&mut self.cookie_changes);
    }

    pub fn take_cookie_changes(&mut self) -> Vec<NativeCookieChange> {
        std::mem::take(&mut self.cookie_changes)
    }

    pub(crate) fn restore_cookie_changes(&mut self, changes: Vec<NativeCookieChange>) {
        self.cookie_changes.extend(changes);
    }

    pub fn apply_cookie_changes(
        &mut self,
        changes: &[NativeCookieChange],
    ) -> Result<(), NativeEngineError> {
        if !self.network.cookie_authority_enabled {
            return Err(NativeEngineError::Worker {
                operation: "apply content-process cookie changes".into(),
                reason: "cookie changes must be applied by the browser process".into(),
            });
        }
        for change in changes {
            match &change.cookie {
                Some(profile) => {
                    let Some(cookie) = NativeCookie::from_profile(profile.clone())? else {
                        self.network
                            .remove_cookie(&change.name, &change.domain, &change.path);
                        self.cookie_changes.push(change.clone());
                        continue;
                    };
                    self.network.set_cookie_profile(cookie)?;
                }
                None => {
                    self.network
                        .remove_cookie(&change.name, &change.domain, &change.path);
                }
            }
            self.cookie_changes.push(change.clone());
        }
        Ok(())
    }

    pub(crate) fn document_cookie(&self, document_url: &str) -> Result<String, NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "document.cookie owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(String::new());
        }
        reject_credentials(&document_url)?;
        Ok(self
            .network
            .document_cookie_header(&document_url)
            .unwrap_or_default())
    }

    pub(crate) fn set_document_cookie(
        &mut self,
        document_url: &str,
        value: &str,
    ) -> Result<(), NativeEngineError> {
        validate_url_text("document URL", document_url)?;
        validate_url_text("document.cookie value", value)?;
        let document_url = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "document.cookie owner URL is not valid URL syntax".into(),
            }
        })?;
        if !is_network_url(document_url.as_str()) {
            return Ok(());
        }
        reject_credentials(&document_url)?;
        if value
            .split(';')
            .skip(1)
            .any(|attribute| attribute.trim().eq_ignore_ascii_case("httponly"))
        {
            return Ok(());
        }
        self.cookie_changes
            .extend(self.network.store_cookie(&document_url, value));
        Ok(())
    }
}
