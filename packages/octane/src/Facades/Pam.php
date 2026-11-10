<?php

declare(strict_types=1);

namespace Pam\Octane\Facades;

use Illuminate\Support\Facades\Facade;
use Pam\Octane\Cache\PageCache;

/**
 * @method static int|null purge(string|list<string> ...$tags)
 * @method static int|null purgeUrl(string|list<string> ...$urls)
 * @method static int|null purgeAll()
 *
 * @see PageCache
 */
final class Pam extends Facade
{
    protected static function getFacadeAccessor(): string
    {
        return PageCache::class;
    }
}
