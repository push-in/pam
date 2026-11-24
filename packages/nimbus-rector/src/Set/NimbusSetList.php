<?php

declare(strict_types=1);

namespace Nimbus\Rector\Set;

/** Rector sets of pushinbr/nimbus-rector, for `RectorConfig::configure()->withSets([...])`. */
final class NimbusSetList
{
    /** Laravel application code to Nimbus: class renames, Request calls, response(), Route facade, abort(). */
    public const string LARAVEL_TO_NIMBUS = __DIR__ . '/../../config/sets/laravel-to-nimbus.php';

    private function __construct()
    {
    }
}
