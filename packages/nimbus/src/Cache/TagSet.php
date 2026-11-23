<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/**
 * The current version of each tag, kept in the store. Flushing a tag gives it a new
 * version, so keys written under the old one are never read again and expire on their own.
 */
final readonly class TagSet
{
    /** @var list<string> */
    private array $names;

    /** @param list<string> $names */
    public function __construct(private Store $store, array $names)
    {
        foreach ($names as $name) {
            if ($name === '') {
                throw new \InvalidArgumentException('Cache tag names cannot be empty.');
            }
        }
        $this->names = array_values(array_unique($names));
    }

    /** @return list<string> */
    public function names(): array
    {
        return $this->names;
    }

    /** The key prefix of the current versions of every tag of the set. */
    public function namespace(): string
    {
        $versions = [];
        foreach ($this->names as $name) {
            $versions[] = $this->version($name);
        }
        return hash('xxh128', implode('|', $versions));
    }

    public function flush(): void
    {
        foreach ($this->names as $name) {
            $this->store->put(self::key($name), bin2hex(random_bytes(10)));
        }
    }

    private function version(string $name): string
    {
        $version = $this->store->get(self::key($name));
        if (is_string($version)) {
            return $version;
        }
        $version = bin2hex(random_bytes(10));
        if (!$this->store->add(self::key($name), $version)) {
            // Another request created it first: use theirs.
            $current = $this->store->get(self::key($name));
            return is_string($current) ? $current : $version;
        }
        return $version;
    }

    private static function key(string $name): string
    {
        return 'tag:' . $name . ':version';
    }
}
