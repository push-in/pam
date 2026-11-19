<?php

declare(strict_types=1);

use Nimbus\Session\Session;
use Nimbus\Translation\Translator;
use Nimbus\View\View;
use Nimbus\View\ViewFactory;

// Global helpers of the optional web layer. Each one reads worker-level services (the
// default view factory and translator) or the current fiber's session, never globals
// that a concurrent request could change.

if (!function_exists('view')) {
    /**
     * A view of the default factory (ViewServiceProvider); return it from a route.
     *
     * @param array<string, mixed> $data
     */
    function view(string $name, array $data = []): View
    {
        return ViewFactory::default()->make($name, $data);
    }
}

if (!function_exists('__')) {
    /**
     * Translates a key in the request's locale (TranslationServiceProvider); without a
     * translator, the key with its placeholders replaced.
     *
     * @param array<string, mixed> $replace
     */
    function __(string $key, array $replace = [], ?string $locale = null): string
    {
        $translator = Translator::default();
        if ($translator !== null) {
            return $translator->get($key, $replace, $locale);
        }
        foreach ($replace as $name => $value) {
            $key = str_replace(':' . $name, is_scalar($value) ? (string) $value : '', $key);
        }
        return $key;
    }
}

if (!function_exists('trans')) {
    /** @param array<string, mixed> $replace */
    function trans(string $key, array $replace = [], ?string $locale = null): string
    {
        return __($key, $replace, $locale);
    }
}

if (!function_exists('trans_choice')) {
    /** @param array<string, mixed> $replace */
    function trans_choice(string $key, int|float $count, array $replace = [], ?string $locale = null): string
    {
        $translator = Translator::default();
        return $translator !== null ? $translator->choice($key, $count, $replace, $locale) : __($key, ['count' => $count] + $replace);
    }
}

if (!function_exists('old')) {
    /** Input flashed by the previous request (after a validation redirect). */
    function old(?string $key = null, mixed $default = null): mixed
    {
        return Session::current()?->old($key, $default) ?? $default;
    }
}

if (!function_exists('csrf_token')) {
    function csrf_token(): string
    {
        return Session::current()?->token() ?? throw new LogicException('csrf_token() needs StartSessionMiddleware.');
    }
}
