| Test | Upstream suite (PAM PHP) | Through PAM client | Scope |
|---|---|---|---|
| `ApplicationFactoryTest::test_application_can_be_created` | PASS | PASS | all servers |
| `ApplicationFactoryTest::test_services_can_be_warmed` | PASS | PASS | all servers |
| `AuthenticationFlushingTest::test_authentication_state_is_flushed_across_subsequent_requests` | PASS | PASS | all servers |
| `AuthorizationStateTest::test_authorization_state_is_updated_across_subsequent_requests` | PASS | PASS | all servers |
| `BinaryBootstrapTest::test_it_can_retrieve_base_path_from_environment_variable` | PASS | PASS | all servers |
| `BindingStateTest::test_container_instances_given_to_dependencies_can_be_stale_if_an_old_instance_is_given` | PASS | PASS | all servers |
| `BindingStateTest::test_container_instances_given_to_dependencies_will_be_fresh_if_injected_container_is_used` | PASS | PASS | all servers |
| `BindingStateTest::test_container_instances_given_to_dependencies_will_be_fresh_if_singleton_but_not_resolved_before_boot` | PASS | PASS | all servers |
| `BindingStateTest::test_container_instances_given_to_dependencies_will_be_stale_if_singleton_and_resolved_during_boot` | PASS | PASS | all servers |
| `BindingStateTest::test_default_listeners_are_bound_as_singletons` | PASS | PASS | all servers |
| `BindingStateTest::test_injecting_request_from_bind_will_always_be_fresh_since_sandbox_request_is_rebound` | PASS | PASS | all servers |
| `BindingStateTest::test_injecting_request_from_singleton_can_be_fresh_if_it_is_not_resolved_during_boot` | PASS | PASS | all servers |
| `BindingStateTest::test_injecting_request_from_singleton_can_be_stale_if_it_is_resolved_during_boot` | PASS | PASS | all servers |
| `BroadcastManagerStateTest::test_broadcast_manager_has_fresh_application_instance` | PASS | PASS | all servers |
| `CacheArrayStateTest::test_array_cache_is_flushed_between_requests` | PASS | PASS | all servers |
| `CloseMonologHandlersTest::test_logger_are_closed_after_worker_termination` | PASS | PASS | all servers |
| `CommandTest::test_error` | PASS | PASS | all servers |
| `CommandTest::test_info` | PASS | PASS | all servers |
| `CommandTest::test_request` | PASS | PASS | all servers |
| `CommandTest::test_warn` | PASS | PASS | all servers |
| `ConfigurationSandboxTest::test_config_is_reset_between_requests` | PASS | PASS | all servers |
| `EnsureRequestsDontExceedMaxExecutionTimeTest::test_process_is_killed_if_current_request_exceeds_max_execution_time` | PASS | PASS | all servers |
| `FilesystemManagerStateTest::test_filesystem_manager_has_fresh_application_instance` | PASS | PASS | all servers |
| `FlushLogContextTest::test_shared_context_is_flushed` | PASS | PASS | all servers |
| `FlushMonologStateTest::test_logger_are_reset` | PASS | PASS | all servers |
| `FlushOnceTest::test_once_is_flushed` | PASS | PASS | all servers |
| `FlushOnceTest::test_once_is_flushed_when_previous_request_was_not_terminated` | PASS | PASS | all servers |
| `FlushStrCacheTest::test_str_is_flushed` | PASS | PASS | all servers |
| `FlushTranslatorCacheTest::test_parsed_keys_cache_is_flushed` | PASS | PASS | all servers |
| `FlushUploadedFilesTest::test_files_removed` | PASS | PASS | all servers |
| `FlushViteTest::test_it_flushes_vite` | PASS | PASS | all servers |
| `FlushViteTest::test_it_handles_instances_of_vite_without_flush_method` | PASS | PASS | all servers |
| `FrankenPhpBinaryRequirementsTest::test_outdated_binary_in_project_root_is_replaced` | PASS | PASS | all servers |
| `FrankenPhpBinaryRequirementsTest::test_outdated_binary_on_path_is_left_untouched` | PASS | PASS | all servers |
| `FrankenPhpClientTest::test_marshal_request` | PASS | PASS | all servers |
| `FrankenPhpClientTest::test_response` | PASS | PASS | all servers |
| `FrankenPhpClientTest::test_streamed_generator_response_flushes_each_yielded_chunk` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_hub_accepts_updates_published_over_http` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_hub_follows_the_connection_claims_and_secrets` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_hub_is_configured_from_the_mercure_broadcasting_connection` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_hubs_on_another_origin_are_ignored` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_mercure_is_disabled_by_default` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_octane_mercure_configuration_can_disable_the_hub` | PASS | PASS | all servers |
| `FrankenPhpMercureConfigTest::test_octane_mercure_configuration_takes_precedence` | PASS | PASS | all servers |
| `FrankenPhpServerStateFileTest::test_server_state_file_can_be_managed` | PASS | PASS | all servers |
| `GiveNewApplicationInstanceToLogManagerTest::test_context_is_appened_to_logs` | PASS | PASS | all servers |
| `GiveNewApplicationInstanceToRouterTest::test_router_has_new_instance` | PASS | PASS | all servers |
| `GiveNewApplicationInstanceToRouterTest::test_router_has_new_instance_when_compiled_route_name_cache_is_warmed` | PASS | PASS | all servers |
| `GiveNewApplicationInstanceToRouterTest::test_router_has_new_instance_when_routes_are_compiled` | PASS | PASS | all servers |
| `InstallCommandTest::test_octane_server_variable_is_appended_when_missing` | PASS | PASS | all servers |
| `InstallCommandTest::test_octane_server_variable_is_updated_when_already_present` | PASS | PASS | all servers |
| `InvokeTickCallableTest::test_callable_is_invoked_when_due` | PASS | PASS | swoole-only |
| `InvokeTickCallableTest::test_callable_is_invoked_when_first_run_and_immediate` | PASS | PASS | swoole-only |
| `InvokeTickCallableTest::test_callable_is_not_invoked_when_first_run_and_not_immediate` | PASS | PASS | swoole-only |
| `InvokeTickCallableTest::test_callable_is_not_invoked_when_not_due` | PASS | PASS | swoole-only |
| `LocaleStateTest::test_carbon_and_app_locale_gets_updated_when_changing_app_locale` | PASS | PASS | all servers |
| `LocaleStateTest::test_carbon_state_is_reset_across_subsequent_requests` | PASS | PASS | all servers |
| `LocaleStateTest::test_translator_state_is_reset_across_subsequent_requests` | PASS | PASS | all servers |
| `MailManagerStateTest::test_mail_manager_has_fresh_application_instance` | PASS | PASS | all servers |
| `NotificationChannelManagerStateTest::test_notification_channel_manager_has_fresh_application_instance` | PASS | PASS | all servers |
| `OctaneStoreTest::test_can_forget_cache_items` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_can_retrieve_items_from_store` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_expired_items_return_null` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_forever_stores_value_in_table` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_get_method_can_resolve_pending_interval` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_increment_and_decrement_operations` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_intervals_are_not_flushed` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_intervals_can_be_refreshed` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_many_method_can_return_many_values` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_missing_items_return_null` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_put_many_stores_value_in_table` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_put_stores_value_in_table` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_touch_returns_false_for_expired_items` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_touch_returns_false_for_missing_items` | SKIP | SKIP | swoole-only |
| `OctaneStoreTest::test_touch_updates_expiration` | SKIP | SKIP | swoole-only |
| `OnWorkerStartTest::test_should_clear_opcache_returns_configured_value` | PASS | PASS | all servers |
| `OnWorkerStartTest::test_should_clear_opcache_returns_true_by_default` | PASS | PASS | all servers |
| `PaginationStateTest::test_pagination_state_is_updated_across_subsequent_requests` | PASS | PASS | all servers |
| `PamClientTest::test_error_method_sends_detailed_error_response_to_pam_in_debug_mode` | PASS | PASS | all servers |
| `PamClientTest::test_error_method_sends_error_response_to_pam` | PASS | PASS | all servers |
| `PamClientTest::test_marshal_request` | PASS | PASS | all servers |
| `PamClientTest::test_marshal_request_keeps_the_raw_body` | PASS | PASS | all servers |
| `PamClientTest::test_response` | PASS | PASS | all servers |
| `PamClientTest::test_streamed_generator_response_flushes_each_yielded_chunk` | PASS | PASS | all servers |
| `PamOctaneStoreTest::test_can_forget_cache_items` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_can_retrieve_items_from_store` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_expired_items_return_null` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_forever_stores_value_in_table` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_get_method_can_resolve_pending_interval` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_increment_and_decrement_operations` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_intervals_are_not_flushed` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_intervals_can_be_refreshed` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_many_method_can_return_many_values` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_missing_items_return_null` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_put_many_stores_value_in_table` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_put_stores_value_in_table` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_touch_returns_false_for_expired_items` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_touch_returns_false_for_missing_items` | PASS | PASS | swoole-only |
| `PamOctaneStoreTest::test_touch_updates_expiration` | PASS | PASS | swoole-only |
| `PamTableTest::test_can_use_all_available_rows` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #0` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #1` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #2` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #3` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #4` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #5` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes with data set #6` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes_only_on_declared_columns` | PASS | PASS | swoole-only |
| `PamTableTest::test_ensures_string_sizes_only_on_string_types` | PASS | PASS | swoole-only |
| `PamTableTest::test_it_gets_used_while_creating_an_octane_store` | PASS | PASS | swoole-only |
| `PamTableTest::test_it_gets_used_while_creating_an_table` | PASS | PASS | swoole-only |
| `PamTableTest::test_it_gets_used_while_creating_an_timer_table` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #0` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #1` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #2` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #3` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #4` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #5` | PASS | PASS | swoole-only |
| `PamTableTest::test_set_of_string_columns with data set #6` | PASS | PASS | swoole-only |
| `PipelineHubStateTest::test_pipeline_hub_has_fresh_application_instance` | PASS | PASS | all servers |
| `QueueManagerStateTest::test_queue_manager_has_fresh_application_instance` | PASS | PASS | all servers |
| `QueuedCookieFlushingTest::test_queued_cookies_from_previous_requests_are_flushed` | PASS | PASS | all servers |
| `RedirectorStateTest::test_redirector_has_correct_request_state_across_subsequent_requests` | PASS | PASS | all servers |
| `ReleaseFreedMemoryTest::test_freed_memory_is_released_to_the_system` | PASS | PASS | all servers |
| `ReportExceptionTest::test_dd_calls_are_not_streamed` | PASS | PASS | all servers |
| `ReportExceptionTest::test_exceptions_are_reported` | PASS | PASS | all servers |
| `ReportExceptionTest::test_exceptions_are_streamed` | PASS | PASS | all servers |
| `RequestSchemeEnforcementTest::test_request_scheme_is_enforced` | PASS | PASS | all servers |
| `RequestServerPortTest::test_request_server_port_is_correct` | PASS | PASS | all servers |
| `RequestStateTest::test_form_requests_contain_the_correct_state_across_subsequent_requests` | PASS | PASS | all servers |
| `RequestStateTest::test_request_is_rebound_on_sandbox` | PASS | PASS | all servers |
| `RequestStateTest::test_request_routes_controller_does_not_leak` | PASS | PASS | all servers |
| `RequestStateTest::test_request_routes_flush_controller_state` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_supported_binary_is_left_untouched with data set #0` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_supported_binary_is_left_untouched with data set #1` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_supported_binary_is_left_untouched with data set #2` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_supported_binary_is_left_untouched with data set #3` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_supported_binary_is_left_untouched with data set #4` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_supported_binary_is_left_untouched with data set #5` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_unsupported_binary_is_replaced with data set #0` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_unsupported_binary_is_replaced with data set #1` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_unsupported_binary_is_replaced with data set #2` | PASS | PASS | all servers |
| `RoadRunnerBinaryRequirementsTest::test_unsupported_binary_is_replaced with data set #3` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_error_method_sends_detailed_error_response_to_roadrunner_in_debug_mode` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_error_method_sends_error_response_to_roadrunner` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_marshal_request_method_marshals_proper_illuminate_request` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_respond_method_ends_streamed_generator_response_quietly_when_roadrunner_stops_it` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_respond_method_send_response_to_roadrunner` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_respond_method_send_streamed_generator_response_to_roadrunner` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_respond_method_send_streamed_response_to_roadrunner` | PASS | PASS | all servers |
| `RoadRunnerClientTest::test_respond_method_send_streamed_untyped_generator_response_to_roadrunner` | PASS | PASS | all servers |
| `RoadRunnerServerProcessInspectorTest::test_can_determine_if_roadrunner_server_process_is_running_when_master_cant_be_communicated_with` | PASS | PASS | all servers |
| `RoadRunnerServerProcessInspectorTest::test_can_determine_if_roadrunner_server_process_is_running_when_master_is_running` | PASS | PASS | all servers |
| `RoadRunnerServerProcessInspectorTest::test_roadrunner_server_process_can_be_reloaded` | PASS | PASS | all servers |
| `RoadRunnerServerStateFileTest::test_server_state_file_can_be_managed` | PASS | PASS | all servers |
| `RouterStateTest::test_router_and_route_container_is_refreshed_across_subsequent_requests` | PASS | PASS | all servers |
| `SequentialCoroutineDispatcherTest::test_coroutines_can_be_resolved` | PASS | PASS | all servers |
| `SequentialTaskDispatcherTest::test_dispatching_tasks_do_not_propagate_exceptions` | PASS | PASS | swoole-only |
| `SequentialTaskDispatcherTest::test_resolving_tasks_propagate_dd_calls` | PASS | PASS | swoole-only |
| `SequentialTaskDispatcherTest::test_resolving_tasks_propagate_exceptions` | PASS | PASS | swoole-only |
| `SequentialTaskDispatcherTest::test_resolving_tasks_with_exceptions_do_not_effect_other_tasks` | PASS | PASS | swoole-only |
| `SequentialTaskDispatcherTest::test_tasks_can_be_dispatched` | PASS | PASS | swoole-only |
| `SequentialTaskDispatcherTest::test_tasks_can_be_resolved` | PASS | PASS | swoole-only |
| `SessionFlushingTest::test_session_is_flushed_between_requests` | PASS | PASS | all servers |
| `StartCommandTest::test_stopping_before_the_input_is_bound_falls_back_to_the_configured_server` | PASS | PASS | all servers |
| `StartRoadRunnerCommandTest::test_configured_absolute_worker_command_is_not_prefixed_with_the_base_path` | PASS | PASS | all servers |
| `StartRoadRunnerCommandTest::test_configured_relative_worker_command_is_resolved_relative_to_the_base_path` | PASS | PASS | all servers |
| `StartRoadRunnerCommandTest::test_default_worker_command_is_resolved_relative_to_the_base_path` | PASS | PASS | all servers |
| `SwooleClientTest::test_can_serve_static_files_if_configured_to_and_file_is_within_public_directory` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_can_serve_static_files_through_symlink` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_can_serve_static_files_when_explicitly_enabled` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_cant_serve_static_files_from_sibling_directory_sharing_public_path_prefix` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_cant_serve_static_files_if_file_has_forbidden_extension` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_cant_serve_static_files_if_file_is_outside_public_directory` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_cant_serve_static_files_through_symlink_using_directory_traversal` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_cant_serve_static_files_when_disabled` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_error_method_sends_detailed_error_response_to_swoole_in_debug_mode` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_error_method_sends_error_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_marshal_request_method_marshals_proper_illuminate_request` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_closes_output_buffer_when_streamed_response_throws` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_preserves_header_formatting_if_configured` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_send_chunked_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_send_not_chunked_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_send_streamed_generator_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_send_streamed_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_sends_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_stops_streamed_generator_response_when_client_disconnects` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_respond_method_with_laravel_specific_status_code_sends_response_to_swoole` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_static_file_can_be_served` | PASS | PASS | swoole-only |
| `SwooleClientTest::test_static_file_headers_can_be_sent` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_container_cpu_count_ignores_invalid_v2_data` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_container_cpu_count_reads_cgroup_v1_quota` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_container_cpu_count_reads_cgroup_v2_quota` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_container_cpu_count_returns_null_when_unlimited_or_missing` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_cpu_count` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_cpu_count_falls_back_when_no_cgroup_limit` | PASS | PASS | swoole-only |
| `SwooleExtensionTest::test_cpu_count_respects_cgroup_v2_limit` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_dispatching_tasks_do_not_propagate_exceptions` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_resolving_tasks_may_timeout` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_resolving_tasks_propagate_dd_calls` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_resolving_tasks_propagate_exceptions` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_tasks_can_be_dispatched_via_fallback_dispatcher` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_tasks_can_be_dispatched_via_http` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_tasks_can_be_resolved_via_fallback_dispatcher` | PASS | PASS | swoole-only |
| `SwooleHttpTaskDispatcherTest::test_tasks_can_be_resolved_via_http` | PASS | PASS | swoole-only |
| `SwooleRequestTest::test_convert_swoole_request_to_illuminate_request` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_cookie` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_delete` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_get` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_patch` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_post` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_put` | SKIP | SKIP | swoole-only |
| `SwooleRequestTest::test_server_variables` | SKIP | SKIP | swoole-only |
| `SwooleServerProcessInspectorTest::test_can_determine_if_swoole_server_process_is_running_when_manager_cant_be_communicated_with` | PASS | PASS | swoole-only |
| `SwooleServerProcessInspectorTest::test_can_determine_if_swoole_server_process_is_running_when_manager_is_running` | PASS | PASS | swoole-only |
| `SwooleServerProcessInspectorTest::test_can_determine_if_swoole_server_process_is_running_when_master_cant_be_communicated_with` | PASS | PASS | swoole-only |
| `SwooleServerProcessInspectorTest::test_can_determine_if_swoole_server_process_is_running_when_only_master_is_running` | PASS | PASS | swoole-only |
| `SwooleServerProcessInspectorTest::test_swoole_server_process_can_be_stop` | PASS | PASS | swoole-only |
| `SwooleServerStateFileTest::test_server_state_file_can_be_managed` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_dispatching_tasks_do_not_propagate_exceptions` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_resolving_tasks_may_timeout` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_resolving_tasks_propagate_dd_calls` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_resolving_tasks_propagate_exceptions` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_tasks_can_be_resolved` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_tasks_can_only_be_dispatched_via_server_context` | PASS | PASS | swoole-only |
| `SwooleTaskDispatcherTest::test_tasks_can_only_be_resolved_via_server_context` | PASS | PASS | swoole-only |
| `TableTest::test_can_use_all_available_rows` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #0` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #1` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #2` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #3` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #4` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #5` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes with data set #6` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes_only_on_declared_columns` | SKIP | SKIP | swoole-only |
| `TableTest::test_ensures_string_sizes_only_on_string_types` | SKIP | SKIP | swoole-only |
| `TableTest::test_it_gets_used_while_creating_an_octane_store` | SKIP | SKIP | swoole-only |
| `TableTest::test_it_gets_used_while_creating_an_table` | SKIP | SKIP | swoole-only |
| `TableTest::test_it_gets_used_while_creating_an_timer_table` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #0` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #1` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #2` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #3` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #4` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #5` | SKIP | SKIP | swoole-only |
| `TableTest::test_set_of_string_columns with data set #6` | SKIP | SKIP | swoole-only |
| `TemporaryContainerInstanceFlushingTest::test_temporary_container_bindings_are_flushed` | PASS | PASS | all servers |
| `TerminalTest::test_width` | PASS | PASS | all servers |
| `UrlGenerationTest::test_url_generator_creates_correct_urls_across_subsequent_requests` | PASS | PASS | all servers |
| `UrlGeneratorSandboxTest::test_url_is_reset_between_requests` | PASS | PASS | all servers |
| `UrlTest::test_url_defaults_are_flushed` | PASS | PASS | all servers |
| `ValidationFactoryStateTest::test_validation_factory_has_fresh_application_instance` | PASS | PASS | all servers |
| `ViewFactoryStateTest::test_shared_view_state_is_persisted_across_subsequent_requests` | PASS | PASS | all servers |
| `ViewFactoryStateTest::test_view_factory_application_is_updated_on_subsequent_requests` | PASS | PASS | all servers |
| `WorkerTest::test_worker_can_dispatch_request_to_application_and_returns_responses_to_client` | PASS | PASS | all servers |
| `WorkerTest::test_worker_can_dispatch_task_to_application_and_returns_responses_to_client` | PASS | PASS | all servers |
| `WorkerTest::test_worker_can_dispatch_ticks_to_application_and_returns_responses_to_client` | PASS | PASS | all servers |
| `WorkerTest::test_worker_doesnt_throw_buffer_error` | PASS | PASS | all servers |
| `WorkerTest::test_worker_releases_the_request_once_it_is_handled` | PASS | PASS | all servers |
| `WorkerTest::test_worker_reports_exceptions_thrown_by_operation_released_listeners` | PASS | PASS | all servers |
