<?php

declare(strict_types=1);

namespace Nimbus\Session;

/**
 * The session of one request. `StartSessionMiddleware` loads it, registers it in the request
 * scope (inject `Session`) and saves it after the response. Flash data lives for the next
 * request; values must be JSON-encodable.
 */
final class Session
{
    private const string TOKEN = '_token';
    private const string FLASH = '_flash';
    private const string OLD_INPUT = '_old_input';
    private const string ERRORS = '_errors';

    /** @var \WeakMap<object, Session>|null */
    private static ?\WeakMap $fibers = null;

    private static ?Session $main = null;

    private bool $dirty = false;

    private ?string $previousId = null;

    /** @param array<string, mixed> $attributes */
    public function __construct(
        private string $id,
        private array $attributes = [],
        private readonly bool $existed = false,
    ) {
        if (!SessionPayload::validId($id)) {
            throw new \InvalidArgumentException('A session id is 40 lowercase hex characters.');
        }
    }

    public static function newId(): string
    {
        return bin2hex(random_bytes(20));
    }

    /** The session of the request running in the current fiber, if any. */
    public static function current(): ?self
    {
        $fiber = \Fiber::getCurrent();
        return $fiber === null ? self::$main : (self::$fibers[$fiber] ?? null);
    }

    /** @internal Registers the session of the current fiber (StartSessionMiddleware). */
    public static function enter(?self $session): void
    {
        $fiber = \Fiber::getCurrent();
        if ($fiber === null) {
            self::$main = $session;
            return;
        }
        self::$fibers ??= new \WeakMap();
        if ($session === null) {
            unset(self::$fibers[$fiber]);
        } else {
            self::$fibers[$fiber] = $session;
        }
    }

    public function id(): string
    {
        return $this->id;
    }

    /** Whether the session came from the store (the browser sent a valid session cookie). */
    public function existed(): bool
    {
        return $this->existed;
    }

    /** The id replaced by regenerate(), destroyed in the store when the session is saved. */
    public function previousId(): ?string
    {
        return $this->previousId;
    }

    public function isDirty(): bool
    {
        return $this->dirty;
    }

    public function get(string $key, mixed $default = null): mixed
    {
        return array_key_exists($key, $this->attributes) ? $this->attributes[$key] : $default;
    }

    public function has(string $key): bool
    {
        return isset($this->attributes[$key]);
    }

    /** @param string|array<string, mixed> $key */
    public function put(string|array $key, mixed $value = null): self
    {
        foreach (is_array($key) ? $key : [$key => $value] as $name => $item) {
            $this->attributes[$name] = $item;
        }
        $this->dirty = true;
        return $this;
    }

    public function forget(string ...$keys): self
    {
        foreach ($keys as $key) {
            unset($this->attributes[$key]);
        }
        $this->dirty = true;
        return $this;
    }

    public function pull(string $key, mixed $default = null): mixed
    {
        $value = $this->get($key, $default);
        $this->forget($key);
        return $value;
    }

    /** @return array<string, mixed> */
    public function all(): array
    {
        return $this->attributes;
    }

    /** Removes everything, the CSRF token included. */
    public function flush(): self
    {
        $this->attributes = [];
        $this->dirty = true;
        return $this;
    }

    /** Stores a value for this request and the next one. */
    public function flash(string $key, mixed $value = true): self
    {
        $this->put($key, $value);
        $flash = $this->flashKeys();
        $flash['new'] = array_values(array_unique([...$flash['new'], $key]));
        $flash['old'] = array_values(array_diff($flash['old'], [$key]));
        $this->attributes[self::FLASH] = $flash;
        return $this;
    }

    /** Stores a value for this request only. */
    public function now(string $key, mixed $value): self
    {
        $this->put($key, $value);
        $flash = $this->flashKeys();
        $flash['old'] = array_values(array_unique([...$flash['old'], $key]));
        $this->attributes[self::FLASH] = $flash;
        return $this;
    }

    /** Keeps all flash data (or the given keys) for one more request. */
    public function reflash(string ...$keys): self
    {
        $flash = $this->flashKeys();
        $keep = $keys === [] ? $flash['old'] : array_values(array_intersect($flash['old'], $keys));
        $flash['new'] = array_values(array_unique([...$flash['new'], ...$keep]));
        $flash['old'] = array_values(array_diff($flash['old'], $keep));
        $this->attributes[self::FLASH] = $flash;
        $this->dirty = true;
        return $this;
    }

    /** @param array<array-key, mixed> $input Flashed as old input; `password*` fields are left out. */
    public function flashInput(array $input): self
    {
        foreach (array_keys($input) as $key) {
            if (is_string($key) && (str_starts_with($key, 'password') || $key === '_token')) {
                unset($input[$key]);
            }
        }
        return $this->flash(self::OLD_INPUT, $input);
    }

    public function old(?string $key = null, mixed $default = null): mixed
    {
        $input = $this->get(self::OLD_INPUT, []);
        if ($key === null) {
            return $input;
        }
        return is_array($input) && array_key_exists($key, $input) ? $input[$key] : $default;
    }

    /** @param array<string, list<string>|string> $errors */
    public function flashErrors(array $errors): self
    {
        return $this->flash(self::ERRORS, $errors);
    }

    public function errors(): ErrorBag
    {
        $errors = $this->get(self::ERRORS, []);
        return new ErrorBag(is_array($errors) ? $errors : []);
    }

    /** The CSRF token of the session, created on first use. */
    public function token(): string
    {
        $token = $this->get(self::TOKEN);
        if (is_string($token) && strlen($token) === 40) {
            return $token;
        }
        return $this->regenerateToken();
    }

    public function regenerateToken(): string
    {
        $token = bin2hex(random_bytes(20));
        $this->put(self::TOKEN, $token);
        return $token;
    }

    /**
     * Moves the data to a new id (call it on login to prevent session fixation);
     * the old id is destroyed in the store when the session is saved.
     */
    public function regenerate(): self
    {
        $this->previousId ??= $this->existed ? $this->id : null;
        $this->id = self::newId();
        $this->dirty = true;
        return $this;
    }

    /** Clears the data and moves to a new id with a new CSRF token (logout). */
    public function invalidate(): self
    {
        $this->flush()->regenerate()->regenerateToken();
        return $this;
    }

    /** @internal Ages flash data at the end of the request (StartSessionMiddleware). */
    public function ageFlashData(): void
    {
        $flash = $this->flashKeys();
        if ($flash['old'] === [] && $flash['new'] === [] && !isset($this->attributes[self::FLASH])) {
            return;
        }
        foreach ($flash['old'] as $key) {
            unset($this->attributes[$key]);
        }
        if ($flash['new'] === []) {
            unset($this->attributes[self::FLASH]);
        } else {
            $this->attributes[self::FLASH] = ['new' => [], 'old' => $flash['new']];
        }
        $this->dirty = true;
    }

    /** @return array{new: list<string>, old: list<string>} */
    private function flashKeys(): array
    {
        $flash = $this->attributes[self::FLASH] ?? [];
        $keys = static fn (mixed $list): array => is_array($list) ? array_values(array_filter($list, is_string(...))) : [];
        return is_array($flash) ? ['new' => $keys($flash['new'] ?? []), 'old' => $keys($flash['old'] ?? [])] : ['new' => [], 'old' => []];
    }
}
