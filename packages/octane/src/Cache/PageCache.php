<?php

declare(strict_types=1);

namespace Pam\Octane\Cache;

use InvalidArgumentException;
use RuntimeException;

/**
 * Purges PAM's edge page cache.
 *
 * Inside a PAM worker the purge is applied at once through the native
 * `pam_cache_purge()` and published to the sibling workers. Anywhere else
 * (queue workers, Artisan, schedulers, PHP-FPM) it is appended to the
 * invalidation log that every PAM worker tails, so it lands within ~100 ms.
 */
final class PageCache
{
    private const TAG_PATTERN = '/\A[A-Za-z0-9\-_.:\/]{1,128}\z/';

    private const LOG_MAX_BYTES = 1024 * 1024;

    public function __construct(private readonly ?string $invalidationLog = null)
    {
    }

    /**
     * Purges every page tagged with any of the given tags.
     *
     * @param string|list<string> ...$tags
     */
    public function purge(string|array ...$tags): ?int
    {
        return $this->send(self::flatten($tags), [], false);
    }

    /**
     * Purges pages by URL: `/products/42`, `/products/*` (prefix) or an
     * absolute `https://host/path?query`. A URL without a query purges every
     * query of that path.
     *
     * @param string|list<string> ...$urls
     */
    public function purgeUrl(string|array ...$urls): ?int
    {
        return $this->send([], self::flatten($urls), false);
    }

    public function purgeAll(): ?int
    {
        return $this->send([], [], true);
    }

    /**
     * Returns the entries purged in this worker, or null when the purge was
     * published through the invalidation log (outside a PAM worker).
     *
     * @param list<string> $tags
     * @param list<string> $urls
     */
    public function send(array $tags, array $urls, bool $all): ?int
    {
        foreach ($tags as $tag) {
            if (preg_match(self::TAG_PATTERN, $tag) !== 1) {
                throw new InvalidArgumentException(
                    "Invalid PAM cache tag [{$tag}]: use 1-128 letters, digits and - _ . : /.",
                );
            }
        }
        foreach ($urls as $url) {
            if (preg_match('/\A[\x21-\x7e]{1,8192}\z/', $url) !== 1
                || !(str_starts_with($url, '/') || preg_match('#\Ahttps?://#i', $url) === 1)) {
                throw new InvalidArgumentException("Invalid PAM cache URL [{$url}].");
            }
        }
        if ($tags === [] && $urls === [] && !$all) {
            return 0;
        }

        if (function_exists('pam_cache_purge')) {
            $purged = pam_cache_purge($tags, $urls, $all);
            if (is_int($purged)) {
                return $purged;
            }
        }

        $this->publish($tags, $urls, $all);

        return null;
    }

    /**
     * @param list<string> $tags
     * @param list<string> $urls
     */
    private function publish(array $tags, array $urls, bool $all): void
    {
        $log = $this->invalidationLog;
        if ($log === null || $log === '') {
            throw new RuntimeException(
                'Cannot purge the PAM page cache outside a PAM worker: configure '
                .'pam-octane.server.responseCacheInvalidationLog.',
            );
        }
        $lines = $all ? "*\n" : '';
        foreach ($tags as $tag) {
            $lines .= $tag."\n";
        }
        foreach ($urls as $url) {
            $lines .= '!url '.$url."\n";
        }

        $directory = dirname($log);
        if (!is_dir($directory) && !@mkdir($directory, 0700, true) && !is_dir($directory)) {
            throw new RuntimeException("Cannot create the PAM invalidation log directory [{$directory}].");
        }
        $previous = umask(0077);
        try {
            $written = @file_put_contents($log, $lines, FILE_APPEND | LOCK_EX);
        } finally {
            umask($previous);
        }
        if ($written !== strlen($lines)) {
            throw new RuntimeException("Cannot write the PAM invalidation log [{$log}].");
        }

        clearstatcache(true, $log);
        if ((int) @filesize($log) > self::LOG_MAX_BYTES) {
            // Workers purge everything when the log is replaced, so no purge
            // can be lost by rotating it.
            $rotated = $log.'.rotate-'.getmypid();
            if (@file_put_contents($rotated, '') !== false) {
                @chmod($rotated, 0600);
                @rename($rotated, $log);
            }
        }
    }

    /**
     * @param array<array-key, string|list<string>> $values
     * @return list<string>
     */
    private static function flatten(array $values): array
    {
        $flat = [];
        array_walk_recursive($values, static function (mixed $value) use (&$flat): void {
            if (!is_string($value)) {
                throw new InvalidArgumentException('PAM cache tags and URLs must be strings.');
            }
            $flat[] = $value;
        });

        return array_values(array_unique($flat));
    }
}
