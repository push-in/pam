<?php

declare(strict_types=1);

namespace Pam\Octane\Cache;

use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\SoftDeletes;

/**
 * Purges PAM's edge page cache when an Eloquent model changes.
 *
 * Pages tagged with the model's table (`products`) or the model itself
 * (`products:42`) are purged after the surrounding transaction commits
 * (immediately outside a transaction). Override `pageCacheTags()` to purge
 * other tags.
 *
 * @mixin Model
 */
trait PurgesPageCache
{
    public static function bootPurgesPageCache(): void
    {
        $purge = static function (Model $model): void {
            if (method_exists($model, 'purgePageCache')) {
                $model->purgePageCache();
            }
        };
        static::saved($purge);
        static::deleted($purge);
        if (in_array(SoftDeletes::class, class_uses_recursive(static::class), true)) {
            static::registerModelEvent('restored', $purge);
        }
    }

    /** @return list<string> */
    public function pageCacheTags(): array
    {
        $table = $this->getTable();
        $key = $this->getKey();

        return is_int($key) || is_string($key) ? [$table, $table.':'.$key] : [$table];
    }

    public function purgePageCache(): void
    {
        $tags = $this->pageCacheTags();
        if ($tags === []) {
            return;
        }
        $purge = static function () use ($tags): void {
            app(PageCache::class)->purge($tags);
        };
        $connection = $this->getConnection();
        if ($connection->transactionLevel() > 0) {
            $connection->afterCommit($purge);

            return;
        }
        $purge();
    }
}
