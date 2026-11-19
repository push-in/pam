<?php

declare(strict_types=1);

namespace Nimbus\Translation;

use Symfony\Component\Translation\Loader\LoaderInterface;
use Symfony\Component\Translation\MessageCatalogue;

/**
 * Loads a locale from a `lang/` directory: `lang/{locale}/{file}.php` returns an array whose
 * keys become `file.key` (nested arrays become dotted keys), and `lang/{locale}.json` maps
 * full strings to translations.
 */
final readonly class LangDirectoryLoader implements LoaderInterface
{
    public function load(mixed $resource, string $locale, string $domain = 'messages'): MessageCatalogue
    {
        if (!is_string($resource) || !is_dir($resource)) {
            throw new \InvalidArgumentException('The translation resource must be a lang directory.');
        }
        $messages = [];
        foreach (self::localeNames($locale) as $name) {
            foreach (glob($resource . '/' . $name . '/*.php') ?: [] as $file) {
                $lines = (static fn (string $path): mixed => require $path)($file);
                if (is_array($lines)) {
                    self::flatten($messages, basename($file, '.php'), $lines);
                }
            }
            $json = $resource . '/' . $name . '.json';
            if (is_file($json)) {
                $decoded = json_decode((string) file_get_contents($json), true, 64, JSON_THROW_ON_ERROR);
                foreach (is_array($decoded) ? $decoded : [] as $key => $value) {
                    if (is_string($value)) {
                        $messages[(string) $key] = $value;
                    }
                }
            }
        }
        return new MessageCatalogue($locale, [$domain => $messages]);
    }

    /**
     * Locales with files in the directory (`pt_BR` for `pt_BR/` or `pt-BR.json`).
     *
     * @return list<string>
     */
    public static function locales(string $directory): array
    {
        $locales = [];
        foreach (glob($directory . '/*') ?: [] as $entry) {
            $name = basename($entry);
            if (is_dir($entry) || str_ends_with($name, '.json')) {
                $code = Locale::normalize(is_dir($entry) ? $name : substr($name, 0, -5));
                if (preg_match('/^[a-z]{2,3}(_[A-Za-z0-9]{2,8})*$/', $code) === 1) {
                    $locales[$code] = true;
                }
            }
        }
        $codes = array_map('strval', array_keys($locales));
        sort($codes);
        return $codes;
    }

    /** @return list<string> */
    private static function localeNames(string $locale): array
    {
        return array_values(array_unique([$locale, str_replace('_', '-', $locale), strtolower(str_replace('_', '-', $locale))]));
    }

    /**
     * @param array<string, string> $messages
     * @param array<array-key, mixed> $lines
     */
    private static function flatten(array &$messages, string $prefix, array $lines): void
    {
        foreach ($lines as $key => $value) {
            if (is_array($value)) {
                self::flatten($messages, $prefix . '.' . $key, $value);
            } elseif (is_scalar($value)) {
                $messages[$prefix . '.' . $key] = (string) $value;
            }
        }
    }
}
