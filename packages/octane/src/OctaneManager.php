<?php

declare(strict_types=1);

namespace Pam\Octane;

use Exception;
use Laravel\Octane\Octane;

/**
 * The `octane` service (the Octane facade) inside a PAM worker:
 * `Octane::table()` returns the PAM shared-memory tables instead of
 * requiring a Swoole server. Released Octane declares a `Swoole\Table`
 * return type, which PAM workers alias to PamTable (PamOctane::aliasTables()).
 */
class OctaneManager extends Octane
{
    /** A manager with the routes already registered on the given instance. */
    public static function from(Octane $octane): self
    {
        $manager = new self();
        $manager->routes = $octane->routes;

        return $manager;
    }

    public function table(string $table): \Swoole\Table
    {
        if (!app()->bound(PamOctane::TABLES)) {
            return parent::table($table);
        }

        $tables = app(PamOctane::TABLES);

        if (!is_array($tables) || !isset($tables[$table])) {
            throw new Exception("Swoole table [{$table}] has not been configured.");
        }

        /** @phpstan-ignore return.type (Swoole\Table is an alias of PamTable here) */
        return $tables[$table];
    }
}
