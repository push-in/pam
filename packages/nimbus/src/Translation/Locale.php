<?php

declare(strict_types=1);

namespace Nimbus\Translation;

/** The locale of the current request, registered in the request scope by LocaleMiddleware. */
final readonly class Locale
{
    public function __construct(public string $code)
    {
        if (preg_match('/^[a-zA-Z]{2,3}([_-][a-zA-Z0-9]{2,8})*$/', $code) !== 1) {
            throw new \InvalidArgumentException("Invalid locale \"{$code}\".");
        }
    }

    /** `pt-br` and `pt_BR` are both `pt_BR`. */
    public static function normalize(string $code): string
    {
        $parts = preg_split('/[_-]/', trim($code)) ?: [];
        $language = strtolower(array_shift($parts) ?? '');
        foreach ($parts as $part) {
            $language .= '_' . (strlen($part) === 2 ? strtoupper($part) : ucfirst(strtolower($part)));
        }
        return $language;
    }
}
