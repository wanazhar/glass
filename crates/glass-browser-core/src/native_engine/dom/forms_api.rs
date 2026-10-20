use super::*;

impl NativeDocument {
    pub(crate) fn form_submission_target(
        &self,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
    ) -> Result<String, NativeEngineError> {
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form submission target is not a form".into(),
            });
        }
        let submitter_node = submitter.and_then(|id| self.node(id));
        let raw_target = submitter_node
            .and_then(|node| node.attribute("formtarget"))
            .or_else(|| form.attribute("target"))
            .unwrap_or("_self")
            .trim();
        if raw_target.is_empty() {
            return Ok("_self".into());
        }
        validate_url_text("form target", raw_target)?;
        validate_window_name(raw_target)?;
        let lower = raw_target.to_ascii_lowercase();
        Ok(match lower.as_str() {
            "_self" | "_parent" | "_top" | "_blank" | "_unfencedtop" => lower,
            _ => raw_target.to_owned(),
        })
    }

    pub(crate) fn form_submission_request(
        &self,
        id: NativeNodeId,
        document_url: &str,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        self.form_submission_request_with_submitter(id, document_url, None)
    }

    pub(crate) fn form_submission_request_with_submitter(
        &self,
        id: NativeNodeId,
        document_url: &str,
        submitter: Option<NativeNodeId>,
    ) -> Result<NativeNavigationRequest, NativeEngineError> {
        let form_id = if self
            .node(id)
            .is_some_and(|node| node.element_name() == Some("form"))
        {
            id
        } else {
            self.submit_control_form(id)
                .ok_or(NativeEngineError::DetachedTarget)?
        };
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "script form submission target is not a form".into(),
            });
        }
        if let Some(submitter) = submitter
            && self.submit_control_form(submitter) != Some(form_id)
        {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form submitter must be a submit control for the form".into(),
            });
        }
        let form_target = self.form_submission_target(form_id, submitter)?;
        let submitter_node = submitter.and_then(|id| self.node(id));
        let method = submitter_node
            .and_then(|node| node.attribute("formmethod"))
            .unwrap_or_else(|| form.attribute("method").unwrap_or("get"));
        let method = if method.eq_ignore_ascii_case("get") {
            super::super::resource_loader::NativeNavigationMethod::Get
        } else if method.eq_ignore_ascii_case("post") {
            super::super::resource_loader::NativeNavigationMethod::Post
        } else {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "native form submission supports only GET and POST".into(),
            });
        };
        let encoding = if method == super::super::resource_loader::NativeNavigationMethod::Post {
            let enctype = submitter_node
                .and_then(|node| node.attribute("formenctype"))
                .unwrap_or_else(|| {
                    form.attribute("enctype")
                        .unwrap_or("application/x-www-form-urlencoded")
                });
            let media_type = enctype.split(';').next().unwrap_or_default().trim();
            if media_type.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
                NativeFormEncoding::UrlEncoded
            } else if media_type.eq_ignore_ascii_case("multipart/form-data") {
                NativeFormEncoding::Multipart
            } else if media_type.eq_ignore_ascii_case("text/plain") {
                NativeFormEncoding::TextPlain
            } else {
                return Err(NativeEngineError::UnsupportedUrl {
                    reason: "native POST form submission supports only urlencoded, multipart, or text/plain encoding".into(),
                });
            }
        } else {
            NativeFormEncoding::UrlEncoded
        };
        let action = submitter_node
            .and_then(|node| node.attribute("formaction"))
            .unwrap_or_else(|| form.attribute("action").unwrap_or(document_url));
        validate_url_text("form action", action)?;
        let base = Url::parse(without_fragment(document_url)).map_err(|_| {
            NativeEngineError::UnsupportedUrl {
                reason: "form submission owner URL is not valid URL syntax".into(),
            }
        })?;
        let mut target = if let Ok(absolute) = Url::parse(action) {
            absolute
        } else {
            base.join(action)
                .map_err(|_| NativeEngineError::UnsupportedUrl {
                    reason: "form action could not be resolved against the document".into(),
                })?
        };
        if !target.username().is_empty() || target.password().is_some() {
            return Err(NativeEngineError::UnsupportedUrl {
                reason: "form action must not contain userinfo credentials".into(),
            });
        }
        let mut pairs = Vec::new();
        self.collect_form_data(form_id, &mut pairs, submitter)?;
        let mut request = match method {
            super::super::resource_loader::NativeNavigationMethod::Get => {
                let query = encode_urlencoded_form_data(&pairs)?;
                if query.is_empty() {
                    target.set_query(None);
                } else {
                    target.set_query(Some(&query));
                }
                let target = target.to_string();
                validate_url_text("form submission URL", &target)?;
                Ok(NativeNavigationRequest::get(target))
            }
            super::super::resource_loader::NativeNavigationMethod::Post => {
                let target = target.to_string();
                validate_url_text("form submission URL", &target)?;
                let (body, content_type) = encode_form_data(&pairs, encoding)?;
                NativeNavigationRequest::post_with_body(target, body, content_type)
            }
            _ => unreachable!("form method was validated above"),
        }?;
        request.target = Some(form_target);
        Ok(request)
    }

    pub(crate) fn submit_control_form(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        let tag_name = node.element_name()?;
        let is_submit = match tag_name {
            "button" => node
                .attribute("type")
                .is_none_or(|kind| kind.eq_ignore_ascii_case("submit")),
            "input" => node.attribute("type").is_some_and(|kind| {
                kind.eq_ignore_ascii_case("submit") || kind.eq_ignore_ascii_case("image")
            }),
            _ => false,
        };
        if !is_submit {
            return None;
        }
        self.form_owner(id)
    }

    pub(crate) fn reset_control_form(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        let tag_name = node.element_name()?;
        let is_reset = match tag_name {
            "button" | "input" => node
                .attribute("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("reset")),
            _ => false,
        };
        if !is_reset {
            return None;
        }
        self.form_owner(id)
    }

    pub(crate) fn has_native_keyboard_button_activation(&self, id: NativeNodeId) -> bool {
        if !self.is_attached(id) || self.is_disabled(id) {
            return false;
        }
        let Some(node) = self.node(id) else {
            return false;
        };
        match node.element_name() {
            Some("button") => true,
            Some("input") => node.attribute("type").is_some_and(|kind| {
                ["button", "reset", "submit", "image"]
                    .iter()
                    .any(|supported| kind.eq_ignore_ascii_case(supported))
            }),
            _ => false,
        }
    }

    pub(crate) fn has_native_keyboard_checkable_activation(&self, id: NativeNodeId) -> bool {
        self.keyboard_checkable_kind(id).is_some()
    }

    pub(crate) fn keyboard_checkable_kind(&self, id: NativeNodeId) -> Option<NativeCheckableKind> {
        if !self.is_attached(id) || self.is_disabled(id) {
            return None;
        }
        self.connected_checkable_kind(id)
    }

    pub(crate) fn connected_checkable_kind(&self, id: NativeNodeId) -> Option<NativeCheckableKind> {
        if !self.is_attached(id) {
            return None;
        }
        let node = self.node(id)?;
        if node.element_name() != Some("input") {
            return None;
        }
        match node.attribute("type")? {
            kind if kind.eq_ignore_ascii_case("checkbox") => Some(NativeCheckableKind::Checkbox),
            kind if kind.eq_ignore_ascii_case("radio") => Some(NativeCheckableKind::Radio),
            _ => None,
        }
    }

    pub(crate) fn pre_activate_checkable(
        &mut self,
        id: NativeNodeId,
    ) -> Result<Option<NativeCheckablePreActivation>, NativeEngineError> {
        let Some(kind) = self.keyboard_checkable_kind(id) else {
            return Ok(None);
        };
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        let checked = node.state.checked;
        let indeterminate = node.state.indeterminate;
        let radio_group_checked = if kind == NativeCheckableKind::Radio {
            self.radio_group_members(id)
                .into_iter()
                .map(|radio_id| {
                    self.node(radio_id)
                        .map(|radio| (radio_id, radio.state.checked))
                        .ok_or(NativeEngineError::DetachedTarget)
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };
        match kind {
            NativeCheckableKind::Checkbox => {
                let state = &mut self
                    .node_mut(id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state;
                state.checked = !state.checked;
                state.indeterminate = false;
            }
            NativeCheckableKind::Radio => {
                let mut radio_ids = self.radio_group_members(id);
                if !radio_ids.contains(&id) {
                    radio_ids.push(id);
                }
                for radio_id in radio_ids {
                    self.node_mut(radio_id)
                        .ok_or(NativeEngineError::DetachedTarget)?
                        .state
                        .checked = radio_id == id;
                }
            }
        }
        Ok(Some(NativeCheckablePreActivation {
            node_id: id,
            kind,
            checked,
            indeterminate,
            radio_group_checked,
        }))
    }

    pub(crate) fn restore_checkable_pre_activation(
        &mut self,
        activation: NativeCheckablePreActivation,
    ) -> Result<(), NativeEngineError> {
        if activation.kind == NativeCheckableKind::Checkbox {
            let state = &mut self
                .node_mut(activation.node_id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state;
            state.checked = activation.checked;
            state.indeterminate = activation.indeterminate;
        } else {
            self.node_mut(activation.node_id)
                .ok_or(NativeEngineError::DetachedTarget)?
                .state
                .indeterminate = activation.indeterminate;
        }
        if activation.kind == NativeCheckableKind::Radio {
            let current_group = self.radio_group_members(activation.node_id);
            let original_checked_in_current_group =
                activation
                    .radio_group_checked
                    .iter()
                    .find_map(|(radio_id, checked)| {
                        (*checked && current_group.contains(radio_id)).then_some(*radio_id)
                    });
            if let Some(checked_id) = original_checked_in_current_group {
                for radio_id in current_group {
                    self.node_mut(radio_id)
                        .ok_or(NativeEngineError::DetachedTarget)?
                        .state
                        .checked = radio_id == checked_id;
                }
            } else {
                self.node_mut(activation.node_id)
                    .ok_or(NativeEngineError::DetachedTarget)?
                    .state
                    .checked = false;
            }
        }
        Ok(())
    }

    pub(super) fn radio_group_members(&self, id: NativeNodeId) -> Vec<NativeNodeId> {
        let Some(node) = self.node(id) else {
            return Vec::new();
        };
        if !self.is_attached(id)
            || node.element_name() != Some("input")
            || !node
                .attribute("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
        {
            return Vec::new();
        }
        let Some(name) = node.attribute("name").filter(|name| !name.is_empty()) else {
            return Vec::new();
        };
        let form_owner = self.form_owner(id);
        let mut members = Vec::new();
        self.collect_radio_group_members(self.root, name, form_owner, &mut members);
        members
    }

    fn collect_radio_group_members(
        &self,
        current: NativeNodeId,
        name: &str,
        form_owner: Option<NativeNodeId>,
        members: &mut Vec<NativeNodeId>,
    ) {
        let Some(node) = self.node(current) else {
            return;
        };
        if node.element_name() == Some("input")
            && node
                .attribute("type")
                .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
            && node.attribute("name") == Some(name)
            && self.form_owner(current) == form_owner
        {
            members.push(current);
        }
        for child in node.children() {
            self.collect_radio_group_members(*child, name, form_owner, members);
        }
    }

    pub(crate) fn has_native_keyboard_link_activation(&self, id: NativeNodeId) -> bool {
        if !self.is_attached(id) {
            return false;
        }
        match self.node(id).and_then(NativeNode::element_name) {
            Some("a") => self
                .node(id)
                .is_some_and(|node| node.attribute("href").is_some()),
            Some("area") => !self.image_map_area_focus_anchors(id).is_empty(),
            _ => false,
        }
    }

    pub(crate) fn reset_form_controls(
        &mut self,
        form_id: NativeNodeId,
    ) -> Result<(), NativeEngineError> {
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form reset target is not a form".into(),
            });
        }
        let controls = self.form_controls_in_document_order(form_id)?;
        for id in &controls {
            let Some(node) = self.node(*id) else {
                continue;
            };
            match node.element_name() {
                Some("input") => {
                    let value = if node
                        .attribute("type")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("file"))
                    {
                        String::new()
                    } else {
                        node.attribute("value").unwrap_or_default().to_owned()
                    };
                    let checked = node.attribute("checked").is_some();
                    let node = self
                        .node_mut(*id)
                        .ok_or(NativeEngineError::DetachedTarget)?;
                    node.state.value = Some(value);
                    node.state.files.clear();
                    node.state.checked = checked;
                    node.state.user_interacted = false;
                    node.state.selection_start = None;
                    node.state.selection_end = None;
                    node.state.selection_direction = None;
                }
                Some("textarea") => {
                    let mut value = String::new();
                    self.collect_raw_text(*id, &mut value);
                    let node = self
                        .node_mut(*id)
                        .ok_or(NativeEngineError::DetachedTarget)?;
                    node.state.value = Some(value);
                    node.state.user_interacted = false;
                    node.state.selection_start = None;
                    node.state.selection_end = None;
                    node.state.selection_direction = None;
                }
                Some("select") => {
                    let options = self.select_option_ids(*id);
                    let selected = options
                        .iter()
                        .copied()
                        .filter(|option_id| {
                            self.node(*option_id)
                                .is_some_and(|option| option.attribute("selected").is_some())
                        })
                        .collect::<Vec<_>>();
                    let multiple = self
                        .node(*id)
                        .is_some_and(|select| select.attribute("multiple").is_some());
                    let selected = if multiple {
                        selected
                            .into_iter()
                            .map(NativeNodeId::index)
                            .collect::<BTreeSet<_>>()
                    } else if let Some(last_selected) = selected.last().copied() {
                        BTreeSet::from([last_selected.index()])
                    } else if self.select_display_size_is_one(*id) {
                        options
                            .iter()
                            .copied()
                            .find(|option_id| !self.is_option_disabled(*option_id))
                            .map(NativeNodeId::index)
                            .into_iter()
                            .collect::<BTreeSet<_>>()
                    } else {
                        BTreeSet::new()
                    };
                    for option_id in options {
                        if let Some(option) = self.node_mut(option_id) {
                            option.state.selected = selected.contains(&option_id.index());
                        }
                    }
                    if let Some(select) = self.node_mut(*id) {
                        select.state.user_interacted = false;
                    }
                }
                _ => {}
            }
        }

        // A radio group can have multiple checked content attributes. Reset
        // each group's last checked control in document order wins.
        let mut checked_radio_groups = BTreeSet::new();
        for id in controls.iter().rev().copied() {
            let Some(node) = self.node(id) else {
                continue;
            };
            if node.element_name() != Some("input")
                || !node
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
                || !node.state.checked
            {
                continue;
            }
            let Some(group) = node
                .attribute("name")
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
            else {
                continue;
            };
            if !checked_radio_groups.insert(group.clone())
                && let Some(node) = self.node_mut(id)
            {
                node.state.checked = false;
            }
        }
        Ok(())
    }

    /// Return required controls that block an interactive form submission.
    /// This is deliberately a bounded validity subset: disabled controls and
    /// read-only controls do not participate, while required text controls,
    /// checkboxes, radio groups, selects, and textareas do.
    pub(crate) fn invalid_form_controls(
        &self,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        self.invalid_form_controls_with_policy(form_id, submitter, true)
    }

    pub(crate) fn invalid_form_controls_for_api(
        &self,
        form_id: NativeNodeId,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        self.invalid_form_controls_with_policy(form_id, None, false)
    }

    fn invalid_form_controls_with_policy(
        &self,
        form_id: NativeNodeId,
        submitter: Option<NativeNodeId>,
        honor_submit_bypass: bool,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        let form = self
            .node(form_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if form.element_name() != Some("form") {
            return Err(NativeEngineError::TargetNotActionable {
                reason: "form validation target is not a form".into(),
            });
        }
        if honor_submit_bypass
            && (form.attribute("novalidate").is_some()
                || submitter.is_some_and(|id| {
                    self.node(id)
                        .is_some_and(|node| node.attribute("formnovalidate").is_some())
                }))
        {
            return Ok(Vec::new());
        }
        let controls = self.form_controls_in_document_order(form_id)?;
        let mut invalid = Vec::new();
        for id in controls {
            let Some(_node) = self.node(id) else {
                continue;
            };
            if self.is_disabled(id) || self.is_read_only(id) {
                continue;
            }
            let valid = self.control_validity(id).0.valid;
            if !valid {
                if invalid.len() >= MAX_FORM_CONTROLS {
                    return Err(NativeEngineError::limit(
                        "invalid form controls",
                        MAX_FORM_CONTROLS,
                        invalid.len().saturating_add(1),
                    ));
                }
                invalid.push(id);
            }
        }
        Ok(invalid)
    }

    pub(crate) fn css_is_focused(&self, id: NativeNodeId) -> bool {
        self.is_attached(id) && self.node(id).is_some_and(|node| node.state.focused)
    }

    pub(crate) fn css_is_hovered(&self, id: NativeNodeId) -> bool {
        self.is_attached(id) && self.node(id).is_some_and(|node| node.state.hovered)
    }

    pub(crate) fn css_focus_within(&self, id: NativeNodeId) -> bool {
        if !self.is_attached(id) {
            return false;
        }
        self.nodes
            .iter()
            .filter(|node| self.is_attached(node.id()) && node.state.focused)
            .any(|focused| {
                let mut current = Some(focused.id());
                for _ in 0..=MAX_NATIVE_DOM_DEPTH {
                    if current == Some(id) {
                        return true;
                    }
                    current = current
                        .and_then(|current_id| self.node(current_id).and_then(NativeNode::parent));
                }
                false
            })
    }

    pub(crate) fn css_validity(&self, id: NativeNodeId) -> Option<bool> {
        let element = self.node(id)?.element_name()?;
        if element == "form" {
            return Some(self.script_validation_snapshot(id).0.valid);
        }
        if !matches!(element, "button" | "input" | "select" | "textarea") {
            return None;
        }
        Some(self.control_validity(id).0.valid)
    }
    pub(crate) fn css_user_validity(&self, id: NativeNodeId) -> Option<bool> {
        let valid = self.css_validity(id)?;
        self.node(id)
            .is_some_and(|node| node.state.user_interacted)
            .then_some(valid)
    }
    pub(crate) fn css_range_validity(&self, id: NativeNodeId) -> Option<bool> {
        let node = self.node(id)?;
        if node.element_name() != Some("input") {
            return None;
        }
        let input_type = node
            .attribute("type")
            .unwrap_or("text")
            .to_ascii_lowercase();
        let has_valid_constraint = match input_type.as_str() {
            "number" | "range" => node.attributes().is_some_and(|attributes| {
                attributes
                    .get("min")
                    .is_some_and(|value| value.parse::<f64>().ok().is_some_and(f64::is_finite))
                    || attributes
                        .get("max")
                        .is_some_and(|value| value.parse::<f64>().ok().is_some_and(f64::is_finite))
            }),
            "date" => node.attributes().is_some_and(|attributes| {
                attributes.get("min").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::Date).is_some()
                }) || attributes.get("max").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::Date).is_some()
                })
            }),
            "month" => node.attributes().is_some_and(|attributes| {
                attributes.get("min").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::Month).is_some()
                }) || attributes.get("max").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::Month).is_some()
                })
            }),
            "time" => node.attributes().is_some_and(|attributes| {
                attributes.get("min").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::Time).is_some()
                }) || attributes.get("max").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::Time).is_some()
                })
            }),
            "datetime-local" => node.attributes().is_some_and(|attributes| {
                attributes.get("min").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::DateTimeLocal).is_some()
                }) || attributes.get("max").is_some_and(|value| {
                    parse_temporal_value(value, TemporalInputKind::DateTimeLocal).is_some()
                })
            }),
            _ => false,
        };
        if !has_valid_constraint || self.current_value(id).is_none_or(|value| value.is_empty()) {
            return None;
        }
        let (validity, will_validate, _) = self.control_validity(id);
        will_validate.then_some(!validity.range_underflow && !validity.range_overflow)
    }

    pub(crate) fn css_read_only(&self, id: NativeNodeId) -> Option<bool> {
        self.node(id)?.element_name()?;
        Some(!self.css_is_editable(id))
    }

    pub(crate) fn css_read_write(&self, id: NativeNodeId) -> Option<bool> {
        self.node(id)?.element_name()?;
        Some(self.css_is_editable(id))
    }

    pub(crate) fn css_placeholder_shown(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        let supported = match node.element_name() {
            Some("textarea") => true,
            Some("input") => !matches!(
                node.attribute("type")
                    .unwrap_or("text")
                    .to_ascii_lowercase()
                    .as_str(),
                "hidden"
                    | "button"
                    | "checkbox"
                    | "color"
                    | "file"
                    | "image"
                    | "radio"
                    | "range"
                    | "reset"
                    | "submit"
            ),
            _ => false,
        };
        supported
            && node.attribute("placeholder").is_some()
            && self.current_value(id).is_some_and(|value| value.is_empty())
    }

    pub(crate) fn css_default(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        match node.element_name() {
            Some("input") => node.attribute("checked").is_some(),
            Some("option") => node.attribute("selected").is_some(),
            _ => false,
        }
    }

    pub(crate) fn css_indeterminate(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        match node.element_name() {
            Some("progress") => node.attribute("value").is_none_or(|value| {
                value
                    .parse::<f64>()
                    .ok()
                    .is_none_or(|parsed| !parsed.is_finite() || parsed < 0.0)
            }),
            Some("input")
                if node
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox")) =>
            {
                node.state.indeterminate
            }
            Some("input")
                if node
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("radio")) =>
            {
                !self
                    .radio_group_members(id)
                    .into_iter()
                    .any(|radio_id| self.node(radio_id).is_some_and(|radio| radio.state.checked))
            }
            _ => false,
        }
    }

    fn css_is_editable(&self, id: NativeNodeId) -> bool {
        let Some(node) = self.node(id) else {
            return false;
        };
        if node
            .attribute("contenteditable")
            .is_some_and(|value| value.is_empty() || value.eq_ignore_ascii_case("true"))
        {
            return true;
        }
        if node
            .attribute("contenteditable")
            .is_some_and(|value| value.eq_ignore_ascii_case("false"))
        {
            return false;
        }
        if self.is_disabled(id) || self.is_read_only(id) {
            return false;
        }
        match node.element_name() {
            Some("textarea") => true,
            Some("input") => !matches!(
                node.attribute("type")
                    .unwrap_or("text")
                    .to_ascii_lowercase()
                    .as_str(),
                "button"
                    | "checkbox"
                    | "color"
                    | "hidden"
                    | "image"
                    | "radio"
                    | "range"
                    | "reset"
                    | "submit"
            ),
            _ => false,
        }
    }

    pub(super) fn script_validation_snapshot(
        &self,
        id: NativeNodeId,
    ) -> (NativeValiditySnapshot, String, bool) {
        let Some(node) = self.node(id) else {
            return (NativeValiditySnapshot::default(), String::new(), false);
        };
        if node.element_name() == Some("form") {
            let valid = self
                .invalid_form_controls_for_api(id)
                .is_ok_and(|invalid| invalid.is_empty());
            return (
                NativeValiditySnapshot {
                    valid,
                    ..NativeValiditySnapshot::default()
                },
                String::new(),
                false,
            );
        }
        let (validity, will_validate, message) = self.control_validity(id);
        (validity, message, will_validate)
    }

    fn control_validity(&self, id: NativeNodeId) -> (NativeValiditySnapshot, bool, String) {
        let Some(node) = self.node(id) else {
            return (NativeValiditySnapshot::default(), false, String::new());
        };
        let will_validate = match node.element_name() {
            Some("input") => {
                let input_type = node.attribute("type").unwrap_or("text");
                !matches!(
                    input_type.to_ascii_lowercase().as_str(),
                    "hidden" | "button" | "submit" | "reset" | "image"
                ) && !self.is_disabled(id)
                    && !self.is_read_only(id)
            }
            Some("select") | Some("textarea") => !self.is_disabled(id) && !self.is_read_only(id),
            Some("button") => !self.is_disabled(id),
            _ => false,
        };
        if !will_validate {
            return (
                NativeValiditySnapshot {
                    valid: true,
                    ..NativeValiditySnapshot::default()
                },
                false,
                String::new(),
            );
        }
        let mut validity = match node.element_name() {
            Some("input") => {
                let input_type = node.attribute("type").unwrap_or("text");
                if input_type.eq_ignore_ascii_case("checkbox") {
                    NativeValiditySnapshot {
                        value_missing: node.attribute("required").is_some() && !node.state.checked,
                        ..NativeValiditySnapshot::default()
                    }
                } else if input_type.eq_ignore_ascii_case("radio") {
                    let checked = self
                        .form_owner(id)
                        .is_some_and(|form_id| self.radio_group_has_checked(form_id, id))
                        || node.state.checked;
                    NativeValiditySnapshot {
                        value_missing: node.attribute("required").is_some() && !checked,
                        ..NativeValiditySnapshot::default()
                    }
                } else {
                    let value = self.current_value(id).unwrap_or_default();
                    text_input_validity(node, &value)
                }
            }
            Some("textarea") => {
                let value = self.current_value(id).unwrap_or_default();
                textarea_validity(node, &value)
            }
            Some("select") => NativeValiditySnapshot {
                value_missing: node.attribute("required").is_some()
                    && self.current_value(id).is_none_or(|value| value.is_empty()),
                ..NativeValiditySnapshot::default()
            },
            _ => NativeValiditySnapshot::default(),
        };
        validity.custom_error = !node.state.custom_validity.is_empty();
        validity.valid = !validity.bad_input
            && !validity.custom_error
            && !validity.pattern_mismatch
            && !validity.range_overflow
            && !validity.range_underflow
            && !validity.step_mismatch
            && !validity.too_long
            && !validity.too_short
            && !validity.type_mismatch
            && !validity.value_missing;
        let message = validation_message(&validity, &node.state.custom_validity);
        (validity, true, message)
    }

    fn collect_form_controls(
        &self,
        parent_id: NativeNodeId,
        controls: &mut Vec<NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        self.node(parent_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        self.collect_form_controls_in_tree(self.root, parent_id, controls, 0)
    }

    fn collect_form_controls_in_tree(
        &self,
        current_id: NativeNodeId,
        form_id: NativeNodeId,
        controls: &mut Vec<NativeNodeId>,
        depth: usize,
    ) -> Result<(), NativeEngineError> {
        if depth > self.max_dom_depth {
            return Err(NativeEngineError::limit(
                "DOM depth",
                self.max_dom_depth,
                depth,
            ));
        }
        let node = self
            .node(current_id)
            .ok_or(NativeEngineError::DetachedTarget)?;
        if node.element_name().is_some() && self.form_owner(current_id) == Some(form_id) {
            controls.push(current_id);
            if controls.len() > MAX_FORM_CONTROLS {
                return Err(NativeEngineError::limit(
                    "form controls",
                    MAX_FORM_CONTROLS,
                    controls.len(),
                ));
            }
        }
        for child in node.children() {
            self.collect_form_controls_in_tree(*child, form_id, controls, depth + 1)?;
        }
        Ok(())
    }

    fn form_controls_in_document_order(
        &self,
        form_id: NativeNodeId,
    ) -> Result<Vec<NativeNodeId>, NativeEngineError> {
        let mut controls = Vec::new();
        self.collect_form_controls(form_id, &mut controls)?;
        Ok(controls)
    }

    pub(super) fn form_owner(&self, id: NativeNodeId) -> Option<NativeNodeId> {
        let node = self.node(id)?;
        if !matches!(
            node.element_name(),
            Some("button" | "input" | "select" | "textarea")
        ) {
            return None;
        }
        if let Some(form_reference) = node.attribute("form") {
            if form_reference.is_empty() {
                return None;
            }
            let first_match = self.first_element_by_id_in_tree(self.root, form_reference)?;
            return (first_match.element_name() == Some("form")).then_some(first_match.id());
        }
        self.find_ancestor_element(id, "form")
    }

    fn first_element_by_id_in_tree(
        &self,
        current_id: NativeNodeId,
        wanted: &str,
    ) -> Option<&NativeNode> {
        let node = self.node(current_id)?;
        if node.element_name().is_some() && node.attribute("id") == Some(wanted) {
            return Some(node);
        }
        node.children()
            .iter()
            .find_map(|child| self.first_element_by_id_in_tree(*child, wanted))
    }

    fn radio_group_has_checked(&self, form_id: NativeNodeId, radio_id: NativeNodeId) -> bool {
        let Some(radio) = self.node(radio_id) else {
            return false;
        };
        let name = radio.attribute("name");
        self.nodes.iter().any(|candidate| {
            self.is_attached(candidate.id())
                && candidate.element_name() == Some("input")
                && candidate
                    .attribute("type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("radio"))
                && self.form_owner(candidate.id()) == Some(form_id)
                && candidate.attribute("name") == name
                && !self.is_disabled(candidate.id())
                && candidate.state.checked
        })
    }

    fn collect_form_data(
        &self,
        form_id: NativeNodeId,
        pairs: &mut Vec<(String, NativeFormValue)>,
        submitter: Option<NativeNodeId>,
    ) -> Result<(), NativeEngineError> {
        for child_id in self.form_controls_in_document_order(form_id)? {
            let Some(child) = self.node(child_id) else {
                continue;
            };
            if self.is_disabled(child_id) {
                continue;
            }
            let name = child.attribute("name").filter(|name| !name.is_empty());
            match child.element_name() {
                Some("input") => {
                    let input_type = child.attribute("type").unwrap_or("text");
                    if input_type.eq_ignore_ascii_case("submit")
                        || input_type.eq_ignore_ascii_case("button")
                        || input_type.eq_ignore_ascii_case("reset")
                        || input_type.eq_ignore_ascii_case("image")
                    {
                        if Some(child_id) == submitter && input_type.eq_ignore_ascii_case("submit")
                        {
                            self.append_submitter_data(child_id, pairs)?;
                        }
                        continue;
                    }
                    if input_type.eq_ignore_ascii_case("file") {
                        if let Some(name) = name {
                            if child.state.files.is_empty() {
                                append_form_value(pairs, name, NativeFormValue::EmptyFile)?;
                            } else {
                                for file in &child.state.files {
                                    append_form_value(
                                        pairs,
                                        name,
                                        NativeFormValue::File(file.clone()),
                                    )?;
                                }
                            }
                        }
                        continue;
                    }
                    if (input_type.eq_ignore_ascii_case("checkbox")
                        || input_type.eq_ignore_ascii_case("radio"))
                        && !child.state.checked
                    {
                        continue;
                    }
                    if let Some(name) = name {
                        let value = child
                            .state
                            .value
                            .clone()
                            .or_else(|| child.attribute("value").map(str::to_owned))
                            .unwrap_or_default();
                        append_form_value(pairs, name, NativeFormValue::Text(value))?;
                    }
                }
                Some("textarea") => {
                    if let Some(name) = name {
                        append_form_value(
                            pairs,
                            name,
                            NativeFormValue::Text(self.current_value(child_id).unwrap_or_default()),
                        )?;
                    }
                }
                Some("button") => {
                    let button_type = child.attribute("type").unwrap_or("submit");
                    if Some(child_id) == submitter && button_type.eq_ignore_ascii_case("submit") {
                        self.append_submitter_data(child_id, pairs)?;
                    }
                }
                Some("select") => {
                    if let Some(name) = name {
                        for option_id in self.select_option_ids(child_id) {
                            let Some(option) = self.node(option_id) else {
                                continue;
                            };
                            if !option.state.selected {
                                continue;
                            }
                            let value = option
                                .attribute("value")
                                .map(str::to_owned)
                                .or_else(|| {
                                    self.element_text(option_id, MAX_LOCATOR_BYTES)
                                        .map(|(value, _)| value)
                                })
                                .unwrap_or_default();
                            append_form_value(pairs, name, NativeFormValue::Text(value))?;
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn append_submitter_data(
        &self,
        id: NativeNodeId,
        pairs: &mut Vec<(String, NativeFormValue)>,
    ) -> Result<(), NativeEngineError> {
        let node = self.node(id).ok_or(NativeEngineError::DetachedTarget)?;
        let Some(name) = node.attribute("name").filter(|name| !name.is_empty()) else {
            return Ok(());
        };
        let value = match node.element_name() {
            Some("input") => node
                .state
                .value
                .clone()
                .or_else(|| node.attribute("value").map(str::to_owned))
                .unwrap_or_default(),
            Some("button") => node.attribute("value").unwrap_or_default().to_owned(),
            _ => String::new(),
        };
        append_form_value(pairs, name, NativeFormValue::Text(value))
    }
}
