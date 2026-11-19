<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Push;

/**
 * Platform-neutral push message, mapped to FCM HTTP v1 and APNs payloads.
 */
final class PushMessage
{
    private ?string $title = null;
    private ?string $body = null;
    /** @var array<string, string> */
    private array $data = [];
    private ?int $badge = null;
    private ?string $sound = null;
    private ?string $image = null;
    private ?string $collapseKey = null;
    private ?string $threadId = null;
    private bool $highPriority = true;
    private ?int $ttlSeconds = null;
    private bool $silent = false;

    public static function make(?string $title = null, ?string $body = null): self
    {
        $message = new self();
        $message->title = $title;
        $message->body = $body;
        return $message;
    }

    public function title(string $title): self
    {
        $this->title = $title;
        return $this;
    }

    public function body(string $body): self
    {
        $this->body = $body;
        return $this;
    }

    /** @param array<string, scalar> $data Values are sent as strings (FCM requires it). */
    public function data(array $data): self
    {
        foreach ($data as $key => $value) {
            $this->data[(string) $key] = is_bool($value) ? ($value ? 'true' : 'false') : (string) $value;
        }
        return $this;
    }

    public function badge(int $badge): self
    {
        $this->badge = max(0, $badge);
        return $this;
    }

    public function sound(string $sound = 'default'): self
    {
        $this->sound = $sound;
        return $this;
    }

    public function image(string $url): self
    {
        $this->image = $url;
        return $this;
    }

    /** Newer messages with the same key replace older ones still pending. */
    public function collapseKey(string $key): self
    {
        $this->collapseKey = $key;
        return $this;
    }

    public function threadId(string $threadId): self
    {
        $this->threadId = $threadId;
        return $this;
    }

    /** Normal priority lets the OS batch delivery to save battery. */
    public function normalPriority(): self
    {
        $this->highPriority = false;
        return $this;
    }

    public function ttl(int $seconds): self
    {
        $this->ttlSeconds = max(0, min($seconds, 2_419_200));
        return $this;
    }

    /** Data-only message that wakes the app without an alert. */
    public function silent(): self
    {
        $this->silent = true;
        $this->highPriority = false;
        return $this;
    }

    /** @return array<string, mixed> */
    public function toArray(): array
    {
        return [
            'title' => $this->title,
            'body' => $this->body,
            'data' => $this->data,
            'badge' => $this->badge,
            'sound' => $this->sound,
            'image' => $this->image,
            'collapse_key' => $this->collapseKey,
            'thread_id' => $this->threadId,
            'high_priority' => $this->highPriority,
            'ttl' => $this->ttlSeconds,
            'silent' => $this->silent,
        ];
    }

    /** @param array<array-key, mixed> $data */
    public static function fromArray(array $data): self
    {
        $message = new self();
        $string = static fn (string $key): ?string => is_string($data[$key] ?? null) ? $data[$key] : null;
        $message->title = $string('title');
        $message->body = $string('body');
        $message->sound = $string('sound');
        $message->image = $string('image');
        $message->collapseKey = $string('collapse_key');
        $message->threadId = $string('thread_id');
        $message->badge = is_int($data['badge'] ?? null) ? $data['badge'] : null;
        $message->ttlSeconds = is_int($data['ttl'] ?? null) ? $data['ttl'] : null;
        $message->highPriority = ($data['high_priority'] ?? true) !== false;
        $message->silent = ($data['silent'] ?? false) === true;
        foreach (is_array($data['data'] ?? null) ? $data['data'] : [] as $key => $value) {
            if (is_scalar($value)) {
                $message->data[(string) $key] = (string) $value;
            }
        }
        return $message;
    }

    /** @return array<string, mixed> FCM HTTP v1 `message` without the token. */
    public function toFcm(): array
    {
        $message = [];
        if (!$this->silent && ($this->title !== null || $this->body !== null)) {
            $message['notification'] = array_filter(
                ['title' => $this->title, 'body' => $this->body, 'image' => $this->image],
                static fn (?string $value): bool => $value !== null,
            );
        }
        if ($this->data !== []) {
            $message['data'] = $this->data;
        }
        $android = ['priority' => $this->highPriority ? 'HIGH' : 'NORMAL'];
        if ($this->ttlSeconds !== null) {
            $android['ttl'] = $this->ttlSeconds . 's';
        }
        if ($this->collapseKey !== null) {
            $android['collapse_key'] = $this->collapseKey;
        }
        if ($this->sound !== null && !$this->silent) {
            $android['notification'] = ['sound' => $this->sound];
        }
        $message['android'] = $android;
        $aps = array_filter(
            ['badge' => $this->badge, 'sound' => $this->silent ? null : $this->sound, 'thread-id' => $this->threadId],
            static fn (int|string|null $value): bool => $value !== null,
        );
        if ($this->silent) {
            $aps['content-available'] = 1;
        }
        if ($aps !== []) {
            $message['apns'] = ['payload' => ['aps' => $aps]];
        }
        return $message;
    }

    /** @return array<string, mixed> APNs JSON payload. */
    public function toApns(): array
    {
        $aps = [];
        if ($this->silent) {
            $aps['content-available'] = 1;
        } else {
            $alert = array_filter(
                ['title' => $this->title, 'body' => $this->body],
                static fn (?string $value): bool => $value !== null,
            );
            if ($alert !== []) {
                $aps['alert'] = $alert;
            }
            if ($this->sound !== null) {
                $aps['sound'] = $this->sound;
            }
            if ($this->image !== null) {
                $aps['mutable-content'] = 1;
            }
        }
        if ($this->badge !== null) {
            $aps['badge'] = $this->badge;
        }
        if ($this->threadId !== null) {
            $aps['thread-id'] = $this->threadId;
        }
        $payload = $this->data;
        if ($this->image !== null) {
            $payload['image'] = $this->image;
        }
        return ['aps' => $aps === [] ? new \stdClass() : $aps] + $payload;
    }

    public function isSilent(): bool
    {
        return $this->silent;
    }

    public function isHighPriority(): bool
    {
        return $this->highPriority;
    }

    public function ttlSeconds(): ?int
    {
        return $this->ttlSeconds;
    }

    public function getCollapseKey(): ?string
    {
        return $this->collapseKey;
    }
}
