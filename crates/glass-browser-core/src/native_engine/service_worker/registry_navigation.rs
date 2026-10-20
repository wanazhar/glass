use super::*;

impl NativeServiceWorkerRegistry {
    pub(crate) async fn intercept_navigation(
        &mut self,
        loader: &mut NativeResourceLoader,
        navigation: &NativeNavigationRequest,
        referrer: Option<&str>,
        referrer_policy: NativeFetchReferrerPolicy,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<NativeServiceWorkerNavigationOutcome, NativeEngineError> {
        let target = parse_network_url(
            "service worker navigation URL",
            without_fragment(&navigation.url),
        )?;
        self.activate_waiting_for_navigation(loader, &target, parent_fetch_broker.as_deref_mut())
            .await?;
        let navigation_preload = self
            .navigation_preload_configuration(&target, navigation.method.as_str())?
            .map(|(scope, header_value)| {
                let request_id = self.allocate_navigation_preload_request_id()?;
                Ok(NativeServiceWorkerNavigationPreloadRequest {
                    request_id,
                    scope,
                    header_value,
                })
            })
            .transpose()?;
        let request_referrer = match referrer {
            Some(source) => Some(
                referrer_for_navigation_with_policy(source, &navigation.url, referrer_policy)?
                    .unwrap_or_default(),
            ),
            None => None,
        };
        let outcome = self
            .intercept_fetch(
                loader,
                &navigation.url,
                &navigation.url,
                navigation.method.as_str(),
                BTreeMap::new(),
                navigation.body.clone(),
                navigation.body_content_type.clone(),
                NativeCorsMode::Navigation,
                NativeFetchRedirectMode::Follow,
                None,
                true,
                None,
                request_referrer.as_deref(),
                Some(referrer.unwrap_or_default()),
                Some(referrer_policy.as_str()),
                "document",
                navigation_preload,
                referrer,
                parent_fetch_broker,
                true,
            )
            .await?;
        let response = match outcome {
            NativeServiceWorkerFetchOutcome::Handled(response) => response,
            NativeServiceWorkerFetchOutcome::NotHandled => {
                return Ok(NativeServiceWorkerNavigationOutcome::NotHandled);
            }
            NativeServiceWorkerFetchOutcome::Suspended => {
                return Ok(NativeServiceWorkerNavigationOutcome::Suspended);
            }
        };
        let url = if response.url.is_empty() {
            without_fragment(&navigation.url).to_owned()
        } else {
            response.url.clone()
        };
        let parsed = parse_network_url("service worker navigation response URL", &url)?;
        loader.set_document_content_security_policy_from_pairs(&url, &response.headers)?;
        Ok(NativeServiceWorkerNavigationOutcome::Handled(
            NativeResource {
                url,
                origin: NativeOrigin::from_url(&parsed)?,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            },
        ))
    }

    pub(super) fn navigation_preload_configuration(
        &self,
        target: &Url,
        method: &str,
    ) -> Result<Option<(String, String)>, NativeEngineError> {
        if method != "GET" {
            return Ok(None);
        }
        let Some(scope) = self.matching_scope(target)? else {
            return Ok(None);
        };
        let Some(profile) = self
            .registration_profiles
            .iter()
            .find(|profile| profile.scope == scope)
        else {
            return Ok(None);
        };
        if !profile.navigation_preload_enabled {
            return Ok(None);
        }
        let Some(worker) = self.registrations.get(&scope) else {
            return Ok(None);
        };
        if !worker.runtime.has_service_worker_fetch_listener()? {
            return Ok(None);
        }
        Ok(Some((
            scope,
            profile.navigation_preload_header_value.clone(),
        )))
    }

    pub(super) fn allocate_navigation_preload_request_id(
        &mut self,
    ) -> Result<u32, NativeEngineError> {
        let request_id = self.next_navigation_preload_request_id;
        if request_id == 0 {
            return Err(NativeEngineError::limit(
                "native ServiceWorker navigation preload request ids",
                u32::MAX as usize,
                u32::MAX as usize,
            ));
        }
        self.next_navigation_preload_request_id = request_id.checked_add(1).unwrap_or_default();
        Ok(request_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fetch_is_controlled(
        &self,
        document_url: &str,
        href: &str,
        destination: &str,
    ) -> Result<bool, NativeEngineError> {
        let owner = parse_network_url("service worker fetch owner URL", document_url)?;
        let target =
            resolve_same_origin_or_cross_origin_url(&owner, "service worker fetch URL", href)?;
        let scope = if destination == "document" {
            self.matching_scope(&target)?
        } else if self.current_client_is_controlled(document_url) {
            self.current_client_scope.clone()
        } else {
            None
        };
        Ok(scope.is_some())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn intercept_fetch(
        &mut self,
        loader: &mut NativeResourceLoader,
        document_url: &str,
        href: &str,
        method: &str,
        request_headers: BTreeMap<String, String>,
        body: Option<NativeRequestBody>,
        content_type: Option<String>,
        cors_mode: NativeCorsMode,
        redirect_mode: NativeFetchRedirectMode,
        _timeout: Option<Duration>,
        credentials: bool,
        credentials_mode: Option<NativeFetchCredentialsMode>,
        referrer: Option<&str>,
        referrer_url: Option<&str>,
        referrer_policy: Option<&str>,
        destination: &str,
        navigation_preload: Option<NativeServiceWorkerNavigationPreloadRequest>,
        navigation_referrer: Option<&str>,
        mut parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
        allow_captured_load_fetch: bool,
    ) -> Result<NativeServiceWorkerFetchOutcome, NativeEngineError> {
        let owner = parse_network_url("service worker fetch owner URL", document_url)?;
        let target =
            resolve_same_origin_or_cross_origin_url(&owner, "service worker fetch URL", href)?;
        let is_navigation = destination == "document";
        let scope = if is_navigation {
            self.matching_scope(&target)?
        } else if self.current_client_is_controlled(document_url) {
            self.current_client_scope.clone()
        } else {
            None
        };
        let Some(scope) = scope else {
            return Ok(NativeServiceWorkerFetchOutcome::NotHandled);
        };
        if let Some(preload) = navigation_preload.as_ref()
            && (!is_navigation || method != "GET" || preload.scope != scope)
        {
            return Err(NativeEngineError::invalid(
                "native ServiceWorker navigation preload request",
                "must match an eligible GET navigation and its registration scope",
            ));
        }
        if !is_navigation {
            if destination == "font" {
                loader.enforce_service_worker_font_policy(&owner, &target)?;
            } else {
                loader.enforce_service_worker_connect_policy(&owner, &target)?;
            }
        }
        let client_id = self.current_client_id_for(document_url);
        let controlled = self.current_client_is_controlled(document_url);
        let clients = self.client_states_for_worker(&scope)?;
        let current_client = self
            .client_states
            .iter()
            .find(|state| state.id == client_id)
            .cloned();
        let body_null = body.is_none();
        let (body_text, body_base64) = match body {
            Some(NativeRequestBody::Text(value)) => (Some(value), None),
            Some(NativeRequestBody::Bytes(value) | NativeRequestBody::StreamedBytes(value)) => (
                None,
                Some(base64::engine::general_purpose::STANDARD.encode(value)),
            ),
            None => (None, None),
        };
        let headers = request_headers
            .into_iter()
            .map(|(name, value)| json!([name, value]))
            .collect::<Vec<_>>();
        let client_url = without_fragment(document_url);
        let payload = json!({
            "url": without_fragment(target.as_str()),
            "method": method,
            "headers": headers,
            "body": body_text,
            "bodyBase64": body_base64,
            "bodyNull": body_null,
            "contentType": content_type,
            "mode": if is_navigation {
                "navigate"
            } else {
                cors_mode_text(cors_mode)
            },
            "redirect": redirect_mode_text(redirect_mode),
            "credentials": credentials,
            "credentialsMode": credentials_mode.map(NativeFetchCredentialsMode::as_str),
            "referrer": referrer.unwrap_or("about:client"),
            "referrerUrl": referrer_url.unwrap_or(client_url),
            "referrerPolicy": referrer_policy.unwrap_or(""),
            "destination": destination,
            "clientId": client_id,
            "clientUrl": client_url,
            "clientType": current_client
                .as_ref()
                .map(|state| state.client_type.as_str())
                .unwrap_or("window"),
            "frameType": current_client
                .as_ref()
                .map(|state| state.frame_type.as_str())
                .unwrap_or("top-level"),
            "visibilityState": current_client
                .as_ref()
                .map(|state| state.visibility_state.as_str())
                .unwrap_or("visible"),
            "focused": current_client.as_ref().is_some_and(|state| state.focused),
            "controlled": controlled,
            "clients": clients,
            "navigationPreloadRequestId": navigation_preload
                .as_ref()
                .map(|preload| preload.request_id),
        });
        let Some(worker) = self.registrations.get_mut(&scope) else {
            return Ok(NativeServiceWorkerFetchOutcome::NotHandled);
        };
        worker.runtime.set_service_worker_clients(
            payload
                .get("clients")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default(),
        );
        let mut preload_response = None;
        let evaluation = if let Some(preload) = navigation_preload.as_ref() {
            let worker_id = worker.id;
            let worker_url = worker.script_url.clone();
            let is_module = worker.is_module;
            let preload_referrer_policy = referrer_policy
                .map(NativeFetchReferrerPolicy::parse)
                .transpose()?
                .ok_or_else(|| {
                    NativeEngineError::invalid(
                        "native ServiceWorker navigation preload policy",
                        "must be present for an eligible preload",
                    )
                })?;
            let mut preload_request = Box::pin(fetch_service_worker_navigation_preload(
                loader,
                parent_fetch_broker.as_deref_mut(),
                preload.request_id,
                target.as_str(),
                navigation_referrer,
                preload_referrer_policy,
                &preload.header_value,
            ));
            let mut preload_result = tokio::select! {
                biased;
                result = &mut preload_request => Some(result),
                _ = tokio::task::yield_now() => None,
            };
            let mut evaluation = worker.runtime.evaluate_service_worker_fetch(
                worker_id,
                &worker_url,
                &payload,
                is_module,
            )?;
            if evaluation.top_level_await_pending && !evaluation.commands.is_empty() {
                resolve_service_worker_fetch_cache_commands(
                    worker,
                    &mut evaluation,
                    &mut self.cache_state,
                )?;
            }
            let mut preload_resolved = false;
            let mut timer_turns = 0usize;
            if evaluation.top_level_await_pending && evaluation.commands.is_empty() {
                loop {
                    if take_service_worker_fetch_response(worker, &mut evaluation)? {
                        break;
                    }
                    if !preload_resolved && let Some(result) = preload_result.as_ref() {
                        let resolved = resolve_service_worker_navigation_preload(
                            &worker.runtime,
                            worker_id,
                            &worker_url,
                            preload.request_id,
                            Some(result),
                            is_module,
                        )?;
                        append_service_worker_fetch_evaluation(
                            &mut evaluation,
                            resolved,
                            "native ServiceWorker navigation preload commands",
                        )?;
                        preload_response = result.as_ref().ok().cloned();
                        preload_resolved = true;
                        continue;
                    }
                    if !evaluation.commands.is_empty() {
                        break;
                    }
                    let timer_delay = worker.runtime.next_worker_timer_delay_ms()?;
                    if !preload_resolved {
                        if let Some(timer_delay) = timer_delay {
                            tokio::select! {
                                biased;
                                result = &mut preload_request => {
                                    preload_result = Some(result);
                                }
                                _ = tokio::time::sleep(Duration::from_millis(timer_delay)) => {
                                    run_service_worker_fetch_timer_turn(
                                        worker,
                                        &mut evaluation,
                                        &mut timer_turns,
                                    )?;
                                }
                            }
                        } else {
                            preload_result = Some(preload_request.as_mut().await);
                        }
                    } else if let Some(timer_delay) = timer_delay {
                        tokio::time::sleep(Duration::from_millis(timer_delay)).await;
                        run_service_worker_fetch_timer_turn(
                            worker,
                            &mut evaluation,
                            &mut timer_turns,
                        )?;
                    } else {
                        break;
                    }
                }
            }
            let fetch_handler_responded = !evaluation.top_level_await_pending
                && evaluation.value.get("handled").and_then(Value::as_bool) == Some(true);
            if fetch_handler_responded && preload_result.is_none() {
                preload_result = std::future::poll_fn(|context| {
                    std::task::Poll::Ready(match preload_request.as_mut().poll(context) {
                        std::task::Poll::Ready(result) => Some(result),
                        std::task::Poll::Pending => None,
                    })
                })
                .await;
            }
            if preload_result.is_none() && fetch_handler_responded {
                drop(preload_request);
                if let Some(parent_fetch_broker) = parent_fetch_broker.as_deref_mut() {
                    parent_fetch_broker
                        .cancel_parent_fetch(preload.request_id)
                        .await?;
                }
            } else if preload_result.is_none() {
                preload_result = Some(preload_request.await);
            }
            if !preload_resolved {
                let resolved = resolve_service_worker_navigation_preload(
                    &worker.runtime,
                    worker_id,
                    &worker_url,
                    preload.request_id,
                    preload_result.as_ref(),
                    is_module,
                )?;
                append_service_worker_fetch_evaluation(
                    &mut evaluation,
                    resolved,
                    "native ServiceWorker navigation preload commands",
                )?;
                preload_response = preload_result
                    .as_ref()
                    .and_then(|result| result.as_ref().ok().cloned());
            }
            evaluation
        } else {
            let mut evaluation = worker.runtime.evaluate_service_worker_fetch(
                worker.id,
                &worker.script_url,
                &payload,
                worker.is_module,
            )?;
            if evaluation.top_level_await_pending && evaluation.commands.is_empty() {
                let mut timer_turns = 0usize;
                while evaluation.top_level_await_pending && evaluation.commands.is_empty() {
                    if take_service_worker_fetch_response(worker, &mut evaluation)? {
                        break;
                    }
                    let Some(timer_delay) = worker.runtime.next_worker_timer_delay_ms()? else {
                        break;
                    };
                    tokio::time::sleep(Duration::from_millis(timer_delay)).await;
                    run_service_worker_fetch_timer_turn(worker, &mut evaluation, &mut timer_turns)?;
                }
            }
            evaluation
        };
        let worker_id = worker.id;
        let settlement = settle_service_worker_fetch_with_parent_fetch_broker(
            worker,
            loader,
            evaluation,
            &mut self.cache_state,
            NativeServiceWorkerFetchSettlementContext {
                current_client_id: &client_id,
                fallback_url: target.as_str(),
                pending_open_windows: &mut self.pending_open_windows,
                lifetime_fetch_tasks: &mut self.lifetime_fetch_tasks,
                lifetime_fetch_abort_handles: &mut self.lifetime_fetch_abort_handles,
                authority: NativeServiceWorkerFetchAuthority {
                    parent_fetch_broker,
                    allow_captured_load_fetch,
                },
            },
        )
        .await?;
        let (value, suspended, client_messages) = match settlement {
            NativeServiceWorkerFetchSettlement::Complete {
                value,
                client_messages,
            } => (value, None, client_messages),
            NativeServiceWorkerFetchSettlement::Suspended {
                continuation,
                client_messages,
            } => (Value::Null, Some(continuation), client_messages),
        };
        let clients_claim_requested = worker.clients_claim_requested;
        self.enqueue_client_messages(client_messages)?;
        let message_port_commands = self
            .registrations
            .get(&scope)
            .map(|worker| worker.runtime.take_message_port_commands())
            .unwrap_or_default();
        self.collect_message_port_commands(worker_id, message_port_commands)?;
        if clients_claim_requested {
            self.claim_current_client(&scope)?;
        }
        if let Some(continuation) = suspended {
            let key = (continuation.worker_id, continuation.open_window_request_id);
            if self.pending_fetches.insert(key, continuation).is_some() {
                return Err(NativeEngineError::Worker {
                    operation: "service worker fetch event".into(),
                    reason: "service worker fetch continuation identifier collided".into(),
                });
            }
            return Ok(NativeServiceWorkerFetchOutcome::Suspended);
        }
        if value.get("handled").and_then(Value::as_bool) != Some(true) {
            if let Some(response) = preload_response {
                return Ok(NativeServiceWorkerFetchOutcome::Handled(response));
            }
            return Ok(NativeServiceWorkerFetchOutcome::NotHandled);
        }
        let response = value
            .get("response")
            .ok_or_else(|| NativeEngineError::Worker {
                operation: "decode service worker response".into(),
                reason: "service worker returned no response envelope".into(),
            })?;
        let response = decode_service_worker_response(response, target.as_str())?;
        if !is_navigation {
            if destination == "font" {
                loader.report_service_worker_font_policy(&owner, &target);
            } else {
                loader.report_service_worker_connect_policy(&owner, &target);
            }
        }
        Ok(NativeServiceWorkerFetchOutcome::Handled(response))
    }

    pub(super) fn matching_scope(&self, target: &Url) -> Result<Option<String>, NativeEngineError> {
        let target_origin = NativeOrigin::from_url(target)?;
        Ok(self
            .registrations
            .values()
            .filter_map(|worker| {
                let scope = Url::parse(without_fragment(&worker.scope)).ok()?;
                if NativeOrigin::from_url(&scope).ok()? != target_origin
                    || !service_worker_scope_matches(scope.path(), target.path())
                {
                    return None;
                }
                Some((scope.path().len(), worker.scope.clone()))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, scope)| scope))
    }

    pub(super) fn matching_waiting_scope(
        &self,
        target: &Url,
    ) -> Result<Option<String>, NativeEngineError> {
        let target_origin = NativeOrigin::from_url(target)?;
        Ok(self
            .waiting_workers
            .values()
            .filter_map(|worker| {
                let scope = Url::parse(without_fragment(&worker.scope)).ok()?;
                if NativeOrigin::from_url(&scope).ok()? != target_origin
                    || !service_worker_scope_matches(scope.path(), target.path())
                {
                    return None;
                }
                Some((scope.path().len(), worker.scope.clone()))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, scope)| scope))
    }

    pub(super) async fn activate_waiting_for_navigation(
        &mut self,
        loader: &mut NativeResourceLoader,
        target: &Url,
        parent_fetch_broker: Option<&mut NativeContentFetchBroker<'_>>,
    ) -> Result<(), NativeEngineError> {
        let Some(scope) = self.matching_waiting_scope(target)? else {
            return Ok(());
        };
        let Some(mut worker) = self.waiting_workers.remove(&scope) else {
            return Ok(());
        };
        if let Err(error) = self
            .settle_worker_activate(&mut worker, loader, parent_fetch_broker)
            .await
        {
            self.waiting_workers.insert(scope, worker);
            return Err(error);
        }
        self.commit_activated_worker(&scope, worker)?;
        Ok(())
    }
}
