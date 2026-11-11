<?php

declare(strict_types=1);

namespace Pam\Octane;

use Pam\Octane\Tables\PamTable;
use RuntimeException;

/**
 * PAM counterparts of Octane features that Octane itself only exposes for
 * Swoole. `Octane::table()` declares a `Swoole\Table` return type in released
 * Octane versions, so tables are reached through this class instead.
 */
final class PamOctane
{
    public const TABLES = 'pam.octane.tables';

    /**
     * Inside a PAM worker without the Swoole extension, `Swoole\Table` names
     * PamTable so that `Octane::table()` keeps its declared return type.
     */
    public static function aliasTables(): bool
    {
        if (class_exists('Swoole\Table', false)) {
            return (new \ReflectionClass('Swoole\Table'))->getName() === PamTable::class;
        }

        if (extension_loaded('swoole') || extension_loaded('openswoole')) {
            return false;
        }

        return class_alias(PamTable::class, 'Swoole\Table');
    }

    public static function table(string $name): PamTable
    {
        $tables = app()->bound(self::TABLES) ? app(self::TABLES) : [];
        $table = is_array($tables) ? ($tables[$name] ?? null) : null;
        if (!$table instanceof PamTable) {
            throw new RuntimeException("Octane table [{$name}] has not been configured.");
        }

        return $table;
    }
}
