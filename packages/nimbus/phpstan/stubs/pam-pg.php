<?php

// Signatures of the pam-pg Postgres functions the PAM runtime registers natively
// (native/pam_pg_php.inc in the PAM repository), for static analysis only.

/** @param array<string, string> $options libpq keywords */
function pam_pg_connect(array $options): int {}

function pam_pg_close(int $session): void {}

/**
 * @param array<int|string, mixed> $params
 * @return ($flags is 1 ? int : ($flags is 2 ? array{0: int, 1: list<string>, 2: list<list<mixed>>} : array{0: int, 1: list<string>, 2: list<array<string, mixed>>}))
 */
function pam_pg_execute(int $session, string $sql, array $params = [], int $flags = 0): array|int {}

function pam_pg_release(int $session, string $sql): void {}

/** @param 0|1|2 $operation begin, commit, rollback */
function pam_pg_transaction(int $session, int $operation): void {}

/** @return int<-1, 2> */
function pam_pg_status(int $session): int {}

function pam_pg_parameter(int $session, string $name): ?string {}
