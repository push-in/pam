<?php

declare(strict_types=1);

namespace Nimbus;

/**
 * Emits the runtime notice of a deprecated Nimbus API, in the format of
 * symfony/deprecation-contracts: through `trigger_deprecation()` when that package is
 * installed, else as a silenced `E_USER_DEPRECATED` that error handlers and PHPUnit
 * still collect. Every call site also carries an `@deprecated` tag naming the
 * replacement and the version that removes it (docs/VERSIONING.md).
 *
 * @internal Implementation detail of Nimbus, excluded from the semver promise; use the public API that wraps it.
 */
final class Deprecation
{
    public const string PACKAGE = 'pushinbr/nimbus';

    private function __construct()
    {
    }

    /** @param string $since The first release that deprecates the API, e.g. `1.2`. */
    public static function trigger(string $since, string $message, string|int|float|bool|null ...$arguments): void
    {
        if (function_exists('trigger_deprecation')) {
            trigger_deprecation(self::PACKAGE, $since, $message, ...$arguments);
            return;
        }
        @trigger_error(
            'Since ' . self::PACKAGE . ' ' . $since . ': ' . ($arguments === [] ? $message : vsprintf($message, array_values($arguments))),
            E_USER_DEPRECATED,
        );
    }
}
