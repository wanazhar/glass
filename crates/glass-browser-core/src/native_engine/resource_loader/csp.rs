use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct NativeCspDirectives {
    pub(super) style_sources: Option<Vec<String>>,
    pub(super) style_element_sources: Option<Vec<String>>,
    pub(super) style_attribute_sources: Option<Vec<String>>,
    pub(super) script_sources: Option<Vec<String>>,
    pub(super) script_element_sources: Option<Vec<String>>,
    pub(super) script_attribute_sources: Option<Vec<String>>,
    pub(super) image_sources: Option<Vec<String>>,
    pub(super) font_sources: Option<Vec<String>>,
    pub(super) media_sources: Option<Vec<String>>,
    pub(super) frame_sources: Option<Vec<String>>,
    pub(super) child_sources: Option<Vec<String>>,
    pub(super) connect_sources: Option<Vec<String>>,
    pub(super) worker_sources: Option<Vec<String>>,
    pub(super) form_action_sources: Option<Vec<String>>,
    pub(super) navigate_to_sources: Option<Vec<String>>,
    pub(super) default_sources: Option<Vec<String>>,
    pub(super) report_uris: Vec<String>,
    pub(super) report_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeCspDeclaration {
    pub(super) directives: NativeCspDirectives,
    pub(super) original_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct NativeCspPolicy {
    pub(super) policies: Vec<NativeCspDirectives>,
    pub(super) header_policy_count: usize,
    pub(super) meta_policy_sources: Vec<String>,
    pub(super) rooted_file_self_root: Option<PathBuf>,
    pub(super) report_only_policies: Vec<NativeCspDeclaration>,
    pub(super) reporting_endpoints: BTreeMap<String, Vec<String>>,
}

impl NativeCspDirectives {
    pub(super) fn allows(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow(sources.map(Vec::as_slice), document_url, resource_url)
    }

    pub(super) fn allows_redirect(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow_for_redirect(sources.map(Vec::as_slice), document_url, resource_url)
    }

    pub(super) fn allows_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        let sources = self
            .sources_for(NativeSubresourceKind::Script)
            .or(self.default_sources.as_ref());
        csp_script_sources_allow(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            parser_inserted,
            nonce,
        )
    }

    pub(super) fn allows_rooted_file_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        same_root: bool,
    ) -> bool {
        let sources = self
            .sources_for(NativeSubresourceKind::Script)
            .or(self.default_sources.as_ref());
        csp_script_sources_allow_for_rooted_file(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            same_root,
        )
    }

    pub(super) fn allows_rooted_file_style(
        &self,
        document_url: &Url,
        resource_url: &Url,
        nonce: Option<&str>,
        same_root: bool,
    ) -> bool {
        let sources = self
            .style_element_sources
            .as_deref()
            .or(self.style_sources.as_deref())
            .or(self.default_sources.as_deref());
        csp_sources_allow_for_rooted_file(sources, document_url, resource_url, nonce, same_root)
    }

    pub(super) fn allows_rooted_file_resource(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        same_root: bool,
    ) -> bool {
        let sources = self.sources_for(kind).or(self.default_sources.as_ref());
        csp_sources_allow_for_rooted_file(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            None,
            same_root,
        )
    }

    pub(super) fn allows_script_redirect(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        let sources = self
            .sources_for(NativeSubresourceKind::Script)
            .or(self.default_sources.as_ref());
        csp_script_sources_allow_for_redirect(
            sources.map(Vec::as_slice),
            document_url,
            resource_url,
            parser_inserted,
            nonce,
        )
    }

    pub(super) fn allows_navigation(
        &self,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) -> bool {
        self.navigation_sources_for(kind).is_none_or(|sources| {
            csp_sources_allow(Some(sources.as_slice()), document_url, target_url)
        })
    }

    pub(super) fn navigation_sources_for(
        &self,
        kind: NativeNavigationPolicyKind,
    ) -> Option<&Vec<String>> {
        match kind {
            NativeNavigationPolicyKind::FormAction => self.form_action_sources.as_ref(),
            NativeNavigationPolicyKind::NavigateTo => self.navigate_to_sources.as_ref(),
        }
    }

    pub(super) fn directive_for(
        &self,
        kind: NativeSubresourceKind,
    ) -> Option<(&'static str, &Vec<String>)> {
        let specific = match kind {
            NativeSubresourceKind::Style => self
                .style_element_sources
                .as_ref()
                .map(|sources| ("style-src-elem", sources))
                .or_else(|| {
                    self.style_sources
                        .as_ref()
                        .map(|sources| ("style-src", sources))
                }),
            NativeSubresourceKind::Script => self
                .script_element_sources
                .as_ref()
                .map(|sources| ("script-src-elem", sources))
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
            NativeSubresourceKind::Image => self
                .image_sources
                .as_ref()
                .map(|sources| ("img-src", sources)),
            NativeSubresourceKind::Font => self
                .font_sources
                .as_ref()
                .map(|sources| ("font-src", sources)),
            NativeSubresourceKind::Media => self
                .media_sources
                .as_ref()
                .map(|sources| ("media-src", sources)),
            NativeSubresourceKind::Frame => self
                .frame_sources
                .as_ref()
                .map(|sources| ("frame-src", sources))
                .or_else(|| {
                    self.child_sources
                        .as_ref()
                        .map(|sources| ("child-src", sources))
                }),
            NativeSubresourceKind::Connect => self
                .connect_sources
                .as_ref()
                .map(|sources| ("connect-src", sources)),
            NativeSubresourceKind::Worker => self
                .worker_sources
                .as_ref()
                .map(|sources| ("worker-src", sources))
                .or_else(|| {
                    self.child_sources
                        .as_ref()
                        .map(|sources| ("child-src", sources))
                })
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
        };
        specific.or_else(|| {
            self.default_sources
                .as_ref()
                .map(|sources| ("default-src", sources))
        })
    }

    pub(super) fn sources_for(&self, kind: NativeSubresourceKind) -> Option<&Vec<String>> {
        match kind {
            NativeSubresourceKind::Style => self
                .style_element_sources
                .as_ref()
                .or(self.style_sources.as_ref()),
            NativeSubresourceKind::Script => self
                .script_element_sources
                .as_ref()
                .or(self.script_sources.as_ref()),
            NativeSubresourceKind::Image => self.image_sources.as_ref(),
            NativeSubresourceKind::Font => self.font_sources.as_ref(),
            NativeSubresourceKind::Media => self.media_sources.as_ref(),
            NativeSubresourceKind::Frame => {
                self.frame_sources.as_ref().or(self.child_sources.as_ref())
            }
            NativeSubresourceKind::Connect => self.connect_sources.as_ref(),
            NativeSubresourceKind::Worker => self
                .worker_sources
                .as_ref()
                .or(self.child_sources.as_ref())
                .or(self.script_sources.as_ref()),
        }
    }

    pub(super) fn inline_sources_for(&self, kind: NativeInlineCspKind) -> Option<&Vec<String>> {
        match kind {
            NativeInlineCspKind::ScriptElement => self
                .script_element_sources
                .as_ref()
                .or(self.script_sources.as_ref())
                .or(self.default_sources.as_ref()),
            NativeInlineCspKind::ScriptAttribute => self
                .script_attribute_sources
                .as_ref()
                .or(self.script_sources.as_ref())
                .or(self.default_sources.as_ref()),
            NativeInlineCspKind::StyleElement => self
                .style_element_sources
                .as_ref()
                .or(self.style_sources.as_ref())
                .or(self.default_sources.as_ref()),
            NativeInlineCspKind::StyleAttribute => self
                .style_attribute_sources
                .as_ref()
                .or(self.style_sources.as_ref())
                .or(self.default_sources.as_ref()),
        }
    }

    pub(super) fn inline_directive_for(
        &self,
        kind: NativeInlineCspKind,
    ) -> Option<(&'static str, &Vec<String>)> {
        let specific = match kind {
            NativeInlineCspKind::ScriptElement => self
                .script_element_sources
                .as_ref()
                .map(|sources| ("script-src-elem", sources))
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
            NativeInlineCspKind::ScriptAttribute => self
                .script_attribute_sources
                .as_ref()
                .map(|sources| ("script-src-attr", sources))
                .or_else(|| {
                    self.script_sources
                        .as_ref()
                        .map(|sources| ("script-src", sources))
                }),
            NativeInlineCspKind::StyleElement => self
                .style_element_sources
                .as_ref()
                .map(|sources| ("style-src-elem", sources))
                .or_else(|| {
                    self.style_sources
                        .as_ref()
                        .map(|sources| ("style-src", sources))
                }),
            NativeInlineCspKind::StyleAttribute => self
                .style_attribute_sources
                .as_ref()
                .map(|sources| ("style-src-attr", sources))
                .or_else(|| {
                    self.style_sources
                        .as_ref()
                        .map(|sources| ("style-src", sources))
                }),
        };
        specific.or_else(|| {
            self.default_sources
                .as_ref()
                .map(|sources| ("default-src", sources))
        })
    }

    pub(super) fn allows_inline(
        &self,
        kind: NativeInlineCspKind,
        source: &str,
        nonce: Option<&str>,
    ) -> bool {
        inline_csp_sources_allow(
            self.inline_sources_for(kind).map(Vec::as_slice),
            source,
            nonce,
            matches!(
                kind,
                NativeInlineCspKind::ScriptAttribute | NativeInlineCspKind::StyleAttribute
            ),
            matches!(
                kind,
                NativeInlineCspKind::ScriptElement | NativeInlineCspKind::ScriptAttribute
            ),
        )
    }
}

impl NativeCspPolicy {
    pub(super) fn allows(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows(kind, document_url, resource_url))
    }

    pub(super) fn allows_redirect(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_redirect(kind, document_url, resource_url))
    }

    pub(super) fn allows_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_script(document_url, resource_url, parser_inserted, nonce))
    }

    pub(super) fn allows_rooted_file_script(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        resource_root: Option<&Path>,
    ) -> bool {
        let same_root = self
            .rooted_file_self_root
            .as_deref()
            .is_some_and(|root| resource_root.is_some_and(|resource_root| resource_root == root));
        self.policies.iter().all(|policy| {
            policy.allows_rooted_file_script(
                document_url,
                resource_url,
                parser_inserted,
                nonce,
                same_root,
            )
        })
    }

    pub(super) fn allows_rooted_file_style(
        &self,
        document_url: &Url,
        resource_url: &Url,
        nonce: Option<&str>,
        resource_root: Option<&Path>,
    ) -> bool {
        let same_root = self
            .rooted_file_self_root
            .as_deref()
            .is_some_and(|root| resource_root.is_some_and(|resource_root| resource_root == root));
        self.policies.iter().all(|policy| {
            policy.allows_rooted_file_style(document_url, resource_url, nonce, same_root)
        })
    }

    pub(super) fn allows_rooted_file_resource(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        resource_root: Option<&Path>,
    ) -> bool {
        let same_root = self
            .rooted_file_self_root
            .as_deref()
            .is_some_and(|root| resource_root.is_some_and(|resource_root| resource_root == root));
        self.policies.iter().all(|policy| {
            policy.allows_rooted_file_resource(kind, document_url, resource_url, same_root)
        })
    }

    pub(super) fn allows_script_redirect(
        &self,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> bool {
        self.policies.iter().all(|policy| {
            policy.allows_script_redirect(document_url, resource_url, parser_inserted, nonce)
        })
    }

    pub(super) fn allows_navigation(
        &self,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_navigation(kind, document_url, target_url))
    }

    pub(super) fn report_only_navigation_violations(
        &self,
        kind: NativeNavigationPolicyKind,
        document_url: &Url,
        target_url: &Url,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let sources = declaration.directives.navigation_sources_for(kind)?;
                (!csp_sources_allow(Some(sources.as_slice()), document_url, target_url))
                    .then_some((declaration, kind.directive()))
            })
            .collect()
    }

    pub(super) fn allows_inline(
        &self,
        kind: NativeInlineCspKind,
        source: &str,
        nonce: Option<&str>,
    ) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_inline(kind, source, nonce))
    }

    pub(super) fn frame_source_groups(&self) -> Option<Vec<Vec<String>>> {
        let groups = self
            .policies
            .iter()
            .filter_map(|policy| {
                policy
                    .sources_for(NativeSubresourceKind::Frame)
                    .or(policy.default_sources.as_ref())
                    .cloned()
            })
            .collect::<Vec<_>>();
        (!groups.is_empty()).then_some(groups)
    }

    pub(super) fn navigation_source_groups(
        &self,
        kind: NativeNavigationPolicyKind,
    ) -> Option<Vec<Vec<String>>> {
        let groups = self
            .policies
            .iter()
            .filter_map(|policy| policy.navigation_sources_for(kind).cloned())
            .collect::<Vec<_>>();
        (!groups.is_empty()).then_some(groups)
    }

    pub(super) fn report_only_url_violations(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_url_violations_with_path(
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            false,
        )
    }

    pub(super) fn report_only_url_violations_for_redirect(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_url_violations_with_path(
            kind,
            document_url,
            resource_url,
            parser_inserted,
            nonce,
            true,
        )
    }

    pub(super) fn report_only_url_violations_with_path(
        &self,
        kind: NativeSubresourceKind,
        document_url: &Url,
        resource_url: &Url,
        parser_inserted: bool,
        nonce: Option<&str>,
        ignore_path: bool,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let (directive, sources) = declaration.directives.directive_for(kind)?;
                let allowed = if kind == NativeSubresourceKind::Script {
                    if ignore_path {
                        declaration.directives.allows_script_redirect(
                            document_url,
                            resource_url,
                            parser_inserted,
                            nonce,
                        )
                    } else {
                        declaration.directives.allows_script(
                            document_url,
                            resource_url,
                            parser_inserted,
                            nonce,
                        )
                    }
                } else {
                    let sources = Some(sources.as_slice());
                    if ignore_path {
                        csp_sources_allow_for_redirect(sources, document_url, resource_url)
                    } else {
                        csp_sources_allow(sources, document_url, resource_url)
                    }
                };
                (!allowed).then_some((declaration, directive))
            })
            .collect()
    }

    pub(super) fn report_only_websocket_violations(
        &self,
        document_url: &Url,
        websocket_url: &Url,
        policy_url: &Url,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let (directive, sources) = declaration
                    .directives
                    .directive_for(NativeSubresourceKind::Connect)?;
                let allowed =
                    csp_sources_allow(Some(sources.as_slice()), document_url, websocket_url)
                        || csp_sources_allow(Some(sources.as_slice()), document_url, policy_url);
                (!allowed).then_some((declaration, directive))
            })
            .collect()
    }

    pub(super) fn report_only_inline_violations(
        &self,
        kind: NativeInlineCspKind,
        source: &str,
        nonce: Option<&str>,
    ) -> Vec<(&NativeCspDeclaration, &'static str)> {
        self.report_only_policies
            .iter()
            .filter_map(|declaration| {
                let (directive, sources) = declaration.directives.inline_directive_for(kind)?;
                (!inline_csp_sources_allow(
                    Some(sources.as_slice()),
                    source,
                    nonce,
                    matches!(
                        kind,
                        NativeInlineCspKind::ScriptAttribute | NativeInlineCspKind::StyleAttribute
                    ),
                    matches!(
                        kind,
                        NativeInlineCspKind::ScriptElement | NativeInlineCspKind::ScriptAttribute
                    ),
                ))
                .then_some((declaration, directive))
            })
            .collect()
    }

    pub(super) fn replace_meta_policies(
        &mut self,
        values: &[String],
    ) -> Result<(), NativeEngineError> {
        if values.len() > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP meta policies",
                MAX_NATIVE_CSP_POLICIES,
                values.len(),
            ));
        }
        if self.header_policy_count.saturating_add(values.len()) > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP policies",
                MAX_NATIVE_CSP_POLICIES,
                self.header_policy_count.saturating_add(values.len()),
            ));
        }
        self.policies.truncate(self.header_policy_count);
        self.policies
            .extend(values.iter().map(|value| parse_csp_directives(value)));
        self.meta_policy_sources = values.to_vec();
        Ok(())
    }

    pub(super) fn append_meta_policies(
        &mut self,
        values: &[String],
    ) -> Result<(), NativeEngineError> {
        if values.len() > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP meta policies",
                MAX_NATIVE_CSP_POLICIES,
                values.len(),
            ));
        }
        let next_len = self.policies.len().saturating_add(values.len());
        if next_len > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP policies",
                MAX_NATIVE_CSP_POLICIES,
                next_len,
            ));
        }
        self.policies
            .extend(values.iter().map(|value| parse_csp_directives(value)));
        self.meta_policy_sources.extend(values.iter().cloned());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct NativeInlineScriptPolicy {
    pub(super) policies: Vec<NativeCspDirectives>,
    pub(super) report_only_policies: Vec<NativeCspDeclaration>,
    pub(super) reporting_endpoints: BTreeMap<String, Vec<String>>,
}

impl NativeInlineScriptPolicy {
    pub(crate) fn append_meta_policy(&mut self, value: &str) -> Result<(), NativeEngineError> {
        let next_len = self.policies.len().saturating_add(1);
        if next_len > MAX_NATIVE_CSP_POLICIES {
            return Err(NativeEngineError::limit(
                "CSP policies",
                MAX_NATIVE_CSP_POLICIES,
                next_len,
            ));
        }
        self.policies.push(parse_csp_directives(value));
        Ok(())
    }

    pub(crate) fn allows(&self, source: &str, nonce: Option<&str>) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_inline(NativeInlineCspKind::ScriptElement, source, nonce))
    }

    pub(crate) fn allows_attribute(&self, source: &str) -> bool {
        self.policies
            .iter()
            .all(|policy| policy.allows_inline(NativeInlineCspKind::ScriptAttribute, source, None))
    }

    pub(crate) fn report_only_inline_script_violations(
        &self,
        document_url: &str,
        source: &str,
        nonce: Option<&str>,
    ) -> Vec<NativeCspViolation> {
        let Ok(document_url) = Url::parse(without_fragment(document_url)) else {
            return Vec::new();
        };
        if !is_network_url(document_url.as_str()) {
            return Vec::new();
        }
        let sample = source.chars().take(40).collect::<String>();
        let mut violations = Vec::new();
        let mut deliveries = Vec::new();
        for declaration in &self.report_only_policies {
            let Some((directive, sources)) = declaration
                .directives
                .inline_directive_for(NativeInlineCspKind::ScriptElement)
            else {
                continue;
            };
            if inline_csp_sources_allow(Some(sources.as_slice()), source, nonce, false, true) {
                continue;
            }
            if violations.len() >= MAX_NATIVE_CSP_VIOLATIONS {
                break;
            }
            let violation = NativeCspViolation {
                document_uri: without_fragment(document_url.as_str()).to_owned(),
                referrer: String::new(),
                blocked_uri: "inline".into(),
                effective_directive: directive.into(),
                violated_directive: directive.into(),
                original_policy: declaration.original_policy.clone(),
                source_file: String::new(),
                sample: sample.clone(),
                disposition: "report".into(),
                status_code: 0,
                line_number: 0,
                column_number: 0,
            };
            deliveries.extend(csp_report_deliveries_for_declaration(
                &self.reporting_endpoints,
                declaration,
                &document_url,
                &violation,
            ));
            violations.push(violation);
        }
        schedule_native_csp_report_deliveries(deliveries);
        violations
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NativeCspHostSource<'a> {
    scheme: Option<&'a str>,
    host: &'a str,
    port: Option<&'a str>,
    path: Option<&'a str>,
}

/// Evaluate the bounded CSP source-expression grammar shared by document
/// subresources and parent-owned frame navigation. Keeping this matcher in
/// the loader prevents the content worker and frame registry from drifting
/// into different CSP decisions.
pub(crate) fn csp_sources_allow(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
) -> bool {
    csp_sources_allow_with_path(sources, document_url, resource_url, false)
}

/// Evaluate a CSP URL source expression for an HTTP redirect. CSP ignores a
/// host-source path on redirect hops to avoid leaking path information while
/// retaining scheme, host, and port authorization.
pub(crate) fn csp_sources_allow_for_redirect(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
) -> bool {
    csp_sources_allow_with_path(sources, document_url, resource_url, true)
}

pub(super) fn csp_sources_allow_with_path(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    ignore_path: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if sources.is_empty() {
        return false;
    }

    if sources.len() == 1 && sources[0].eq_ignore_ascii_case("'none'") {
        return false;
    }

    for source in sources {
        if source.eq_ignore_ascii_case("'none'") {
            continue;
        }
        if csp_source_expression_matches(source, document_url, resource_url, ignore_path) {
            return true;
        }
    }
    false
}

pub(super) fn csp_script_sources_allow(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
) -> bool {
    csp_script_sources_allow_with_path(
        sources,
        document_url,
        resource_url,
        parser_inserted,
        nonce,
        false,
    )
}

pub(super) fn csp_script_sources_allow_for_redirect(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
) -> bool {
    csp_script_sources_allow_with_path(
        sources,
        document_url,
        resource_url,
        parser_inserted,
        nonce,
        true,
    )
}

pub(super) fn csp_script_sources_allow_with_path(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
    ignore_path: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if nonce.is_some_and(|nonce| {
        sources
            .iter()
            .any(|candidate| csp_nonce_matches(candidate, nonce))
    }) {
        return true;
    }
    if sources
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case("'strict-dynamic'"))
    {
        return !parser_inserted;
    }
    csp_sources_allow_with_path(Some(sources), document_url, resource_url, ignore_path)
}

pub(super) fn csp_sources_allow_for_rooted_file(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    nonce: Option<&str>,
    same_root: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if nonce.is_some_and(|nonce| {
        sources
            .iter()
            .any(|candidate| csp_nonce_matches(candidate, nonce))
    }) {
        return true;
    }
    sources.iter().any(|source| {
        if source.eq_ignore_ascii_case("'none'") {
            return false;
        }
        if source.eq_ignore_ascii_case("'self'") {
            return same_root;
        }
        csp_source_expression_matches(source, document_url, resource_url, false)
    })
}

pub(super) fn csp_script_sources_allow_for_rooted_file(
    sources: Option<&[String]>,
    document_url: &Url,
    resource_url: &Url,
    parser_inserted: bool,
    nonce: Option<&str>,
    same_root: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if nonce.is_some_and(|nonce| {
        sources
            .iter()
            .any(|candidate| csp_nonce_matches(candidate, nonce))
    }) {
        return true;
    }
    if sources
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case("'strict-dynamic'"))
    {
        return !parser_inserted;
    }
    sources.iter().any(|source| {
        if source.eq_ignore_ascii_case("'none'") {
            return false;
        }
        if source.eq_ignore_ascii_case("'self'") {
            return same_root;
        }
        csp_source_expression_matches(source, document_url, resource_url, false)
    })
}

pub(super) fn csp_source_expression_matches(
    expression: &str,
    document_url: &Url,
    resource_url: &Url,
    ignore_path: bool,
) -> bool {
    if expression.is_empty() || expression.len() > MAX_NATIVE_CSP_SOURCE_EXPRESSION_BYTES {
        return false;
    }
    if expression == "*" {
        return matches!(resource_url.scheme(), "http" | "https")
            || resource_url
                .scheme()
                .eq_ignore_ascii_case(document_url.scheme());
    }

    if expression.eq_ignore_ascii_case("'self'") {
        return csp_self_source_matches(document_url, resource_url);
    }

    if let Some(scheme) = expression.strip_suffix(':')
        && !expression.contains("://")
        && csp_scheme_part_is_valid(scheme)
    {
        return csp_scheme_part_matches(scheme, resource_url.scheme());
    }

    let Some(source) = parse_csp_host_source(expression) else {
        return false;
    };
    let Some(resource_host) = resource_url.host_str() else {
        return false;
    };
    let required_scheme = source.scheme.unwrap_or(document_url.scheme());
    if !csp_scheme_part_matches(required_scheme, resource_url.scheme())
        || !csp_host_part_matches(source.host, resource_host)
        || !csp_port_part_matches(source.port, resource_url)
    {
        return false;
    }

    source
        .path
        .is_none_or(|path| ignore_path || csp_path_part_matches(path, resource_url.path()))
}

fn parse_csp_host_source(expression: &str) -> Option<NativeCspHostSource<'_>> {
    if expression.is_empty() || !expression.is_ascii() {
        return None;
    }

    let (scheme, authority_and_path) = if let Some((scheme, rest)) = expression.split_once("://") {
        (Some(scheme), rest)
    } else {
        (None, expression)
    };
    if scheme.is_some_and(|scheme| !csp_scheme_part_is_valid(scheme)) {
        return None;
    }

    let (authority, path) =
        authority_and_path
            .find('/')
            .map_or((authority_and_path, None), |path_start| {
                (
                    &authority_and_path[..path_start],
                    Some(&authority_and_path[path_start..]),
                )
            });
    if authority.is_empty() {
        return None;
    }

    let (host, port) = match authority.rsplit_once(':') {
        None => (authority, None),
        Some((host, port)) => {
            if port.is_empty()
                || (port != "*"
                    && (!port.bytes().all(|byte| byte.is_ascii_digit())
                        || port.parse::<u16>().is_err()))
            {
                return None;
            }
            (host, Some(port))
        }
    };
    if !csp_host_part_is_valid(host) {
        return None;
    }
    if path.is_some_and(|path| !csp_path_part_is_valid(path)) {
        return None;
    }

    Some(NativeCspHostSource {
        scheme,
        host,
        port,
        path,
    })
}

pub(super) fn csp_scheme_part_is_valid(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

pub(super) fn csp_scheme_part_matches(pattern: &str, resource_scheme: &str) -> bool {
    pattern.eq_ignore_ascii_case(resource_scheme)
        || (pattern.eq_ignore_ascii_case("http") && resource_scheme.eq_ignore_ascii_case("https"))
        || (pattern.eq_ignore_ascii_case("ws")
            && matches!(
                resource_scheme.to_ascii_lowercase().as_str(),
                "wss" | "http" | "https"
            ))
        || (pattern.eq_ignore_ascii_case("wss") && resource_scheme.eq_ignore_ascii_case("https"))
}

pub(super) fn csp_host_part_is_valid(value: &str) -> bool {
    if value == "*" {
        return true;
    }
    let host = value.strip_prefix("*.").unwrap_or(value);
    if host.is_empty() || host.contains('*') {
        return false;
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && host.split('.').all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

pub(super) fn csp_host_part_matches(pattern: &str, resource_host: &str) -> bool {
    let pattern = pattern.trim_end_matches('.').to_ascii_lowercase();
    let resource_host = resource_host.trim_end_matches('.').to_ascii_lowercase();
    if pattern == "*" {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return resource_host.len() > suffix.len()
            && resource_host
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'));
    }
    pattern.eq_ignore_ascii_case(&resource_host)
}

pub(super) fn csp_default_port(scheme: &str) -> Option<u16> {
    match scheme.to_ascii_lowercase().as_str() {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        _ => None,
    }
}

pub(super) fn csp_port_part_matches(port: Option<&str>, resource_url: &Url) -> bool {
    if port == Some("*") {
        return true;
    }

    let resource_port = resource_url.port();
    let default_port = csp_default_port(resource_url.scheme());
    let effective_resource_port = resource_port.or(default_port);
    match port {
        None => resource_port.is_none() || resource_port == default_port,
        Some(port) => port
            .parse::<u16>()
            .ok()
            .is_some_and(|port| Some(port) == effective_resource_port),
    }
}

pub(super) fn csp_path_part_is_valid(path: &str) -> bool {
    if !path.starts_with('/') || !path.is_ascii() {
        return false;
    }
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            if index + 2 >= bytes.len()
                || hex_value(bytes[index + 1]).is_none()
                || hex_value(bytes[index + 2]).is_none()
            {
                return false;
            }
            index += 3;
            continue;
        }
        if !(byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'.'
                    | b'_'
                    | b'~'
                    | b'!'
                    | b'$'
                    | b'&'
                    | b'\''
                    | b'('
                    | b')'
                    | b'*'
                    | b'+'
                    | b'='
                    | b':'
                    | b'@'
                    | b'/'
            ))
        {
            return false;
        }
        index += 1;
    }
    true
}

pub(super) fn csp_percent_decode_path_piece(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return None;
        }
        let high = hex_value(bytes[index + 1])?;
        let low = hex_value(bytes[index + 2])?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    Some(decoded)
}

pub(super) fn csp_path_part_matches(pattern: &str, resource_path: &str) -> bool {
    if pattern.is_empty() || (pattern == "/" && resource_path.is_empty()) {
        return true;
    }

    let exact_match = !pattern.ends_with('/');
    let mut pattern_pieces = pattern.split('/').collect::<Vec<_>>();
    let resource_pieces = resource_path.split('/').collect::<Vec<_>>();
    if pattern_pieces.len() > resource_pieces.len()
        || (exact_match && pattern_pieces.len() != resource_pieces.len())
    {
        return false;
    }
    if !exact_match {
        pattern_pieces.pop();
    }

    pattern_pieces
        .iter()
        .zip(resource_pieces.iter())
        .all(|(pattern_piece, resource_piece)| {
            csp_percent_decode_path_piece(pattern_piece)
                == csp_percent_decode_path_piece(resource_piece)
        })
}

pub(super) fn csp_self_source_matches(document_url: &Url, resource_url: &Url) -> bool {
    if resource_url.scheme().eq_ignore_ascii_case("blob") {
        return false;
    }
    if document_url.origin() == resource_url.origin() {
        return true;
    }

    let Some(document_host) = document_url.host_str() else {
        return false;
    };
    let Some(resource_host) = resource_url.host_str() else {
        return false;
    };
    if !document_host.eq_ignore_ascii_case(resource_host)
        || !csp_upgrade_ports_compatible(document_url, resource_url)
    {
        return false;
    }

    matches!(
        (
            document_url.scheme().to_ascii_lowercase().as_str(),
            resource_url.scheme().to_ascii_lowercase().as_str(),
        ),
        ("http" | "https", "https" | "wss") | ("http", "ws")
    )
}

pub(super) fn csp_upgrade_ports_compatible(document_url: &Url, resource_url: &Url) -> bool {
    let document_default = csp_default_port(document_url.scheme());
    let resource_default = csp_default_port(resource_url.scheme());
    let document_effective = document_url.port().or(document_default);
    let resource_effective = resource_url.port().or(resource_default);
    document_effective == resource_effective
        || (document_url
            .port()
            .is_none_or(|port| Some(port) == document_default)
            && resource_url
                .port()
                .is_none_or(|port| Some(port) == resource_default))
}

pub(super) fn inline_csp_sources_allow(
    sources: Option<&[String]>,
    source: &str,
    nonce: Option<&str>,
    style_attribute: bool,
    strict_dynamic: bool,
) -> bool {
    let Some(sources) = sources else {
        return true;
    };
    if sources.is_empty() {
        return false;
    }
    let has_nonce_or_hash = sources
        .iter()
        .any(|source| csp_nonce_or_hash_source(source));
    let strict_dynamic = strict_dynamic
        && sources
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case("'strict-dynamic'"));
    if !has_nonce_or_hash
        && !strict_dynamic
        && sources
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case("'unsafe-inline'"))
    {
        return true;
    }
    if !style_attribute
        && nonce.is_some_and(|nonce| {
            sources
                .iter()
                .any(|candidate| csp_nonce_matches(candidate, nonce))
        })
    {
        return true;
    }
    let unsafe_hashes = sources
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case("'unsafe-hashes'"));
    if style_attribute && !unsafe_hashes {
        return false;
    }
    sources
        .iter()
        .any(|candidate| csp_hash_matches(candidate, source))
}

pub(super) fn csp_nonce_or_hash_source(source: &str) -> bool {
    let source = source.to_ascii_lowercase();
    source.ends_with('\'')
        && ["'nonce-", "'sha256-", "'sha384-", "'sha512-"]
            .iter()
            .any(|prefix| source.starts_with(prefix))
}

pub(super) fn csp_nonce_matches(source: &str, nonce: &str) -> bool {
    let Some(value) = source.strip_suffix('\'').and_then(|source| source.get(7..)) else {
        return false;
    };
    source
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("'nonce-"))
        && value == nonce
}

pub(super) fn csp_hash_matches(source: &str, value: &str) -> bool {
    let Some(source) = source.strip_suffix('\'') else {
        return false;
    };
    let Some((algorithm, expected)) = source.get(1..).and_then(|source| source.split_once('-'))
    else {
        return false;
    };
    let digest = match algorithm.to_ascii_lowercase().as_str() {
        "sha256" => Sha256::digest(value.as_bytes()).to_vec(),
        "sha384" => Sha384::digest(value.as_bytes()).to_vec(),
        "sha512" => Sha512::digest(value.as_bytes()).to_vec(),
        _ => return false,
    };
    base64::engine::general_purpose::STANDARD.encode(digest) == expected
}
