<?php

declare(strict_types=1);

namespace Nimbus\Cache;

/** A cache view whose keys belong to a set of tags; `flush()` invalidates only them. */
final class TaggedCache extends Repository
{
    public function __construct(Store $store, private readonly TagSet $tags)
    {
        parent::__construct($store);
    }

    /** Invalidates every key stored under these tags. */
    public function flush(): void
    {
        $this->tags->flush();
    }

    public function tags(string|array $tags): TaggedCache
    {
        $tags = is_string($tags) ? [$tags] : $tags;
        return new self($this->store, new TagSet($this->store, [...$this->tags->names(), ...$tags]));
    }

    protected function key(string $key): string
    {
        return 'tagged:' . $this->tags->namespace() . ':' . parent::key($key);
    }
}
