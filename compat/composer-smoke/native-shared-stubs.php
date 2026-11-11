<?php

// Signatures of the shared-table and task-worker functions PAM registers
// natively (native/pam_shared_php.inc), for static analysis only.

/** @param list<array{0: string, 1: int, 2: int}> $columns */
function pam_native_table_open(string $name, int $rows, array $columns, int $key_size = 64): int {}

function pam_native_table_close(int $table): bool {}

/** @param array<string, mixed> $values */
function pam_native_table_set(int $table, string $key, array $values): bool {}

/** @return array<string, int|float|string>|int|float|string|false */
function pam_native_table_get(int $table, string $key, ?string $field = null): array|int|float|string|false {}

function pam_native_table_exists(int $table, string $key): bool {}

function pam_native_table_del(int $table, string $key): bool {}

function pam_native_table_incr(int $table, string $key, string $column, int|float $by): int|float|false {}

/** @return int<0, max> */
function pam_native_table_count(int $table): int {}

/** @return array{0: int, 1: list<array{0: string, 1: array<string, int|float|string>}>} */
function pam_native_table_scan(int $table, int $cursor, int $limit = 128): array {}

/** @return array{size: int, memory_size: int, key_size: int, recoveries: int} */
function pam_native_table_info(int $table): array {}

function pam_native_task_available(): bool {}

/**
 * @param list<string> $payloads
 * @return list<string|null>
 */
function pam_native_task_resolve(array $payloads, int $wait_ms): array {}

/** @param list<string> $payloads */
function pam_native_task_dispatch(array $payloads): bool {}

function pam_native_task_flush(int $timeout_ms = 5000): int {}
