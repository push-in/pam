<?php

declare(strict_types=1);

namespace Nimbus\Translation;

use Nimbus\Container\Container;
use Symfony\Component\Translation\Translator as SymfonyTranslator;

/**
 * Translations from a `lang/` directory on symfony/translation. The catalogues are loaded
 * once per worker; the locale is never global state: it comes from the argument, the
 * request's `Locale` (LocaleMiddleware) or the default.
 *
 * Placeholders: `:name` (`:Name`, `:NAME` change case), `{name}` and `%name%`. Plurals use
 * the Symfony/Laravel form `{0} None|{1} One|[2,*] :count items`.
 */
final class Translator
{
    private static ?self $default = null;

    private readonly SymfonyTranslator $translator;

    /** @var list<string> */
    private readonly array $locales;

    /**
     * @param list<string> $fallbackLocales
     * @param list<string>|null $supportedLocales Locales a request may select; defaults to those in the directory.
     */
    public function __construct(
        private readonly string $directory,
        private readonly string $defaultLocale = 'en',
        array $fallbackLocales = [],
        private readonly ?Container $container = null,
        ?array $supportedLocales = null,
    ) {
        if (!class_exists(SymfonyTranslator::class)) {
            throw new \LogicException('Translations require symfony/translation: composer require symfony/translation.');
        }
        $default = Locale::normalize($defaultLocale);
        $this->translator = new SymfonyTranslator($default);
        $this->translator->addLoader('nimbus', new LangDirectoryLoader());
        $available = is_dir($directory) ? LangDirectoryLoader::locales($directory) : [];
        foreach ($available as $locale) {
            $this->translator->addResource('nimbus', $directory, $locale);
        }
        $fallbacks = array_values(array_unique(array_map(Locale::normalize(...), $fallbackLocales === [] ? [$default] : $fallbackLocales)));
        $this->translator->setFallbackLocales($fallbacks);
        $this->locales = array_values(array_unique(array_map(Locale::normalize(...), $supportedLocales ?? ($available === [] ? [$default] : $available))));
    }

    /** The translator used by `__()`, `trans()` and `trans_choice()`. */
    public static function default(): ?self
    {
        return self::$default;
    }

    public function useAsDefault(): self
    {
        self::$default = $this;
        return $this;
    }

    /** The locale of the current request, or the default one. */
    public function locale(): string
    {
        $locale = $this->container?->scopedValue(Locale::class);
        return $locale instanceof Locale ? $locale->code : Locale::normalize($this->defaultLocale);
    }

    /** @return list<string> */
    public function supportedLocales(): array
    {
        return $this->locales;
    }

    public function directory(): string
    {
        return $this->directory;
    }

    /** @param array<string, mixed> $replace */
    public function get(string $key, array $replace = [], ?string $locale = null): string
    {
        return $this->translate($key, $replace, $locale, null);
    }

    /** @param array<string, mixed> $replace */
    public function choice(string $key, int|float $count, array $replace = [], ?string $locale = null): string
    {
        return $this->translate($key, ['count' => $count] + $replace, $locale, $count);
    }

    public function has(string $key, ?string $locale = null): bool
    {
        return $this->translator->getCatalogue(Locale::normalize($locale ?? $this->locale()))->has($key);
    }

    /**
     * The best supported locale for an `Accept-Language` header (q-values honored, `pt-BR`
     * falls back to `pt`), or null.
     */
    public function negotiate(?string $acceptLanguage): ?string
    {
        if ($acceptLanguage === null || trim($acceptLanguage) === '') {
            return null;
        }
        $ranges = [];
        foreach (explode(',', $acceptLanguage, 32) as $index => $range) {
            $parts = explode(';', trim($range));
            $tag = trim($parts[0]);
            $quality = 1.0;
            foreach (array_slice($parts, 1) as $parameter) {
                if (preg_match('/^\s*q=([01](?:\.\d{0,3})?)\s*$/i', $parameter, $match) === 1) {
                    $quality = (float) $match[1];
                }
            }
            if ($tag !== '' && $tag !== '*' && $quality > 0) {
                $ranges[] = [Locale::normalize($tag), $quality, $index];
            }
        }
        usort($ranges, static fn (array $a, array $b): int => [$b[1], $a[2]] <=> [$a[1], $b[2]]);
        $supported = array_combine(array_map('strtolower', $this->locales), $this->locales);
        foreach ($ranges as [$tag]) {
            $exact = $supported[strtolower($tag)] ?? null;
            if ($exact !== null) {
                return $exact;
            }
            $language = strtolower(explode('_', $tag)[0]);
            if (isset($supported[$language])) {
                return $supported[$language];
            }
            foreach ($this->locales as $locale) {
                if (str_starts_with(strtolower($locale), $language . '_')) {
                    return $locale;
                }
            }
        }
        return null;
    }

    public function supports(string $locale): bool
    {
        return in_array(strtolower(Locale::normalize($locale)), array_map('strtolower', $this->locales), true);
    }

    /** @param array<string, mixed> $replace */
    private function translate(string $key, array $replace, ?string $locale, int|float|null $count): string
    {
        $locale = Locale::normalize($locale ?? $this->locale());
        $catalogue = $this->translator->getCatalogue($locale);
        $message = $catalogue->has($key) ? $catalogue->get($key) : null;
        if ($message === null) {
            foreach ($this->translator->getFallbackLocales() as $fallback) {
                $fallbackCatalogue = $this->translator->getCatalogue($fallback);
                if ($fallbackCatalogue->has($key)) {
                    $message = $fallbackCatalogue->get($key);
                    $locale = $fallback;
                    break;
                }
            }
        }
        $message ??= $key;
        if ($count !== null) {
            $message = preg_replace(['/,\s*\*\s*\]/', '/\[\s*\*\s*,/'], [',Inf]', '[-Inf,'], $message) ?? $message;
            $message = $this->translator->trans($message, ['%count%' => $count], 'nimbus-inline', $locale);
        }
        return strtr($message, self::placeholders($replace));
    }

    /**
     * @param array<string, mixed> $replace
     * @return array<string, string>
     */
    private static function placeholders(array $replace): array
    {
        $pairs = [];
        foreach ($replace as $name => $value) {
            $text = match (true) {
                $value === null => '',
                is_scalar($value) => (string) $value,
                $value instanceof \Stringable => (string) $value,
                default => '',
            };
            $pairs[':' . $name] = $text;
            $pairs[':' . ucfirst($name)] = ucfirst($text);
            $pairs[':' . strtoupper($name)] = strtoupper($text);
            $pairs['{' . $name . '}'] = $text;
            $pairs['%' . $name . '%'] = $text;
        }
        return $pairs;
    }
}
