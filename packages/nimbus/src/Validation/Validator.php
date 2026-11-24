<?php

declare(strict_types=1);

namespace Nimbus\Validation;

/**
 * Laravel-style rule validation of decoded input, used by `FormRequest` and on its own:
 *
 * ```php
 * $data = Validator::make($payload, [
 *     'hours' => 'nullable|integer|between:1,720',
 *     'since' => ['required', 'date'],
 *     'items' => 'required|array|min:1',
 *     'items.*.sku' => ['required', 'string', Rule::exists('products', 'sku')],
 * ], ['items.*.sku.exists' => 'Unknown product :value.'])->validate();
 * ```
 *
 * Rules are pipe strings or lists of strings and `ValidationRule` objects. Keys accept dot
 * paths (`customer.email`) and `*` wildcards (`items.*.sku`); errors are reported under
 * the concrete path (`items.0.sku`). The validated data keeps only fields with rules;
 * `integer`, `numeric` and `boolean` values given as strings (query, form data) are cast.
 *
 * Messages follow Laravel's English defaults; `$messages` overrides them by `field.rule`,
 * `field` or `rule` (wildcards allowed), with `:attribute`, `:value`, `:min`, `:max`,
 * `:size`, `:values`, `:other`, `:date` and `:format` placeholders.
 */
final class Validator
{
    private const array IMPLICIT = [
        'required', 'required_if', 'required_unless', 'required_with', 'required_without', 'present', 'accepted',
    ];

    private const array NUMERIC = ['integer', 'numeric'];

    private static ?PresenceVerifier $presenceVerifier = null;

    /** @var array<string, list<string>> */
    private array $errors = [];

    /** @var array<string, mixed> */
    private array $validated = [];

    private bool $ran = false;

    /**
     * @param array<array-key, mixed> $data
     * @param array<string, string|list<string|ValidationRule>> $rules
     * @param array<string, string> $messages
     * @param array<string, string> $attributes Display names of fields in messages.
     */
    public function __construct(
        private readonly array $data,
        private readonly array $rules,
        private readonly array $messages = [],
        private readonly array $attributes = [],
    ) {
    }

    /**
     * @param array<array-key, mixed> $data
     * @param array<string, string|list<string|ValidationRule>> $rules
     * @param array<string, string> $messages
     * @param array<string, string> $attributes
     */
    public static function make(array $data, array $rules, array $messages = [], array $attributes = []): self
    {
        return new self($data, $rules, $messages, $attributes);
    }

    /** The verifier of `exists`/`unique` rules (`ArcanoServiceProvider` registers the Arcano one). */
    public static function usePresenceVerifier(?PresenceVerifier $verifier): void
    {
        self::$presenceVerifier = $verifier;
    }

    public static function presenceVerifier(): PresenceVerifier
    {
        return self::$presenceVerifier ?? throw new \LogicException(
            'The exists and unique rules need a PresenceVerifier; register Nimbus\\Database\\ArcanoServiceProvider '
            . 'or call Validator::usePresenceVerifier().',
        );
    }

    /**
     * The validated data, or a ValidationException with every error.
     *
     * @return array<string, mixed>
     * @throws ValidationException
     */
    public function validate(): array
    {
        $this->run();
        if ($this->errors !== []) {
            throw new ValidationException($this->errors);
        }
        return $this->validated;
    }

    public function fails(): bool
    {
        $this->run();
        return $this->errors !== [];
    }

    public function passes(): bool
    {
        return !$this->fails();
    }

    /** @return array<string, list<string>> */
    public function errors(): array
    {
        $this->run();
        return $this->errors;
    }

    /** @return array<string, mixed> The fields that passed, also when others failed. */
    public function validated(): array
    {
        $this->run();
        return $this->validated;
    }

    /**
     * A rule set as a list: `'required|integer|min:1'` becomes `['required', 'integer', 'min:1']`.
     *
     * @param string|list<string|ValidationRule> $rules
     * @return list<string|ValidationRule>
     */
    public static function parse(string|array $rules): array
    {
        if (is_string($rules)) {
            return $rules === '' ? [] : explode('|', $rules);
        }
        $parsed = [];
        foreach ($rules as $rule) {
            if (is_string($rule) && str_contains($rule, '|') && !str_starts_with($rule, 'regex:') && !str_starts_with($rule, 'not_regex:')) {
                array_push($parsed, ...explode('|', $rule));
                continue;
            }
            $parsed[] = $rule;
        }
        return $parsed;
    }

    private function run(): void
    {
        if ($this->ran) {
            return;
        }
        $this->ran = true;
        foreach ($this->rules as $key => $rules) {
            $rules = self::parse($rules);
            foreach (str_contains($key, '*') ? $this->expand($key) : [$key] as $path) {
                $this->validateAttribute($path, $key, $rules);
            }
        }
    }

    /** @param list<string|ValidationRule> $rules */
    private function validateAttribute(string $path, string $key, array $rules): void
    {
        [$exists, $value] = self::lookup($this->data, $path);
        $names = [];
        foreach ($rules as $rule) {
            if (is_string($rule)) {
                $names[] = explode(':', $rule, 2)[0];
            }
        }
        if (in_array('sometimes', $names, true) && !$exists) {
            return;
        }
        $nullable = in_array('nullable', $names, true);
        $bail = in_array('bail', $names, true);
        $numeric = array_intersect($names, self::NUMERIC) !== [];
        $failed = false;
        foreach ($rules as $rule) {
            if ($rule instanceof ValidationRule) {
                if (!$exists || ($nullable && $value === null)) {
                    continue;
                }
                $message = $rule->validate($path, $value);
                if ($message !== null) {
                    $failed = true;
                    $this->errors[$path][] = $this->custom($path, $key, self::ruleName($rule), $value, [])
                        ?? $message;
                    if ($bail) {
                        break;
                    }
                }
                continue;
            }
            [$name, $parameters] = self::ruleAndParameters($rule);
            if (in_array($name, ['nullable', 'sometimes', 'bail'], true)) {
                continue;
            }
            $implicit = in_array($name, self::IMPLICIT, true);
            if (!$implicit) {
                // Laravel runs only implicit rules on missing fields and empty strings.
                if (!$exists || (is_string($value) && trim($value) === '') || ($nullable && $value === null)) {
                    continue;
                }
            }
            $result = $this->check($name, $parameters, $path, $value, $exists, $numeric);
            if ($result === null) {
                continue;
            }
            [$default, $replacements] = $result;
            $failed = true;
            $this->errors[$path][] = $this->custom($path, $key, $name, $value, $replacements)
                ?? $this->replace($default, $path, $value, $replacements);
            // Laravel stops at a failed implicit rule (required...) or at any failure with bail.
            if ($bail || $implicit) {
                break;
            }
        }
        if (!$failed && $exists) {
            $this->store($path, self::cast($names, $value));
        }
    }

    /**
     * Null when the rule passes, else the default message and its placeholders.
     *
     * @param list<string> $parameters
     * @return ?array{string, array<string, string>}
     */
    private function check(string $name, array $parameters, string $path, mixed $value, bool $exists, bool $numeric): ?array
    {
        $type = self::sizeType($value, $numeric);
        return match ($name) {
            'required' => self::filled($exists, $value) ? null : ['The :attribute field is required.', []],
            'required_if' => $this->requiredIf($parameters, $exists, $value, true),
            'required_unless' => $this->requiredIf($parameters, $exists, $value, false),
            'required_with' => $this->anyPresent($parameters) && !self::filled($exists, $value)
                ? ['The :attribute field is required when :values is present.', ['values' => implode(' / ', $parameters)]]
                : null,
            'required_without' => !$this->allPresent($parameters) && !self::filled($exists, $value)
                ? ['The :attribute field is required when :values is not present.', ['values' => implode(' / ', $parameters)]]
                : null,
            'present' => $exists ? null : ['The :attribute field must be present.', []],
            'filled' => self::filled($exists, $value) ? null : ['The :attribute field must have a value.', []],
            'accepted' => in_array($value, ['yes', 'on', '1', 1, true, 'true'], true) ? null : ['The :attribute field must be accepted.', []],
            'string' => is_string($value) ? null : ['The :attribute field must be a string.', []],
            'integer' => self::isInteger($value) ? null : ['The :attribute field must be an integer.', []],
            'numeric' => self::isNumeric($value) ? null : ['The :attribute field must be a number.', []],
            'boolean' => self::toBoolean($value) !== null ? null : ['The :attribute field must be true or false.', []],
            'array' => is_array($value) ? null : ['The :attribute field must be an array.', []],
            'list' => is_array($value) && array_is_list($value) ? null : ['The :attribute field must be a list.', []],
            'email' => is_string($value) && filter_var($value, FILTER_VALIDATE_EMAIL) !== false
                ? null
                : ['The :attribute field must be a valid email address.', []],
            'url' => is_string($value) && filter_var($value, FILTER_VALIDATE_URL) !== false
                ? null
                : ['The :attribute field must be a valid URL.', []],
            'uuid' => is_string($value) && preg_match('/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/iD', $value) === 1
                ? null
                : ['The :attribute field must be a valid UUID.', []],
            'ip' => is_string($value) && filter_var($value, FILTER_VALIDATE_IP) !== false
                ? null
                : ['The :attribute field must be a valid IP address.', []],
            'json' => is_string($value) && json_validate($value) ? null : ['The :attribute field must be a valid JSON string.', []],
            'alpha' => is_string($value) && preg_match('/^[\pL\pM]+$/uD', $value) === 1
                ? null
                : ['The :attribute field must only contain letters.', []],
            'alpha_num' => is_string($value) && preg_match('/^[\pL\pM\pN]+$/uD', $value) === 1
                ? null
                : ['The :attribute field must only contain letters and numbers.', []],
            'alpha_dash' => is_string($value) && preg_match('/^[\pL\pM\pN_-]+$/uD', $value) === 1
                ? null
                : ['The :attribute field must only contain letters, numbers, dashes, and underscores.', []],
            'regex' => is_string($value) && @preg_match(self::parameter($parameters, 0, $name), $value) === 1
                ? null
                : ['The :attribute field format is invalid.', []],
            'not_regex' => is_string($value) && @preg_match(self::parameter($parameters, 0, $name), $value) === 0
                ? null
                : ['The :attribute field format is invalid.', []],
            'date' => self::isDate($value) ? null : ['The :attribute field must be a valid date.', []],
            'date_format' => self::matchesFormat($value, self::parameter($parameters, 0, $name))
                ? null
                : ['The :attribute field must match the format :format.', ['format' => $parameters[0]]],
            'after', 'after_or_equal', 'before', 'before_or_equal' => $this->compareDate($name, $parameters, $value),
            'in' => self::in($value, $parameters) ? null : ['The selected :attribute is invalid.', []],
            'not_in' => !self::in($value, $parameters) ? null : ['The selected :attribute is invalid.', []],
            'confirmed' => (self::lookup($this->data, $path . '_confirmation')[1] ?? null) === $value
                ? null
                : ['The :attribute field confirmation does not match.', []],
            'same' => self::lookup($this->data, self::parameter($parameters, 0, $name))[1] === $value
                ? null
                : ['The :attribute field must match :other.', ['other' => $parameters[0]]],
            'different' => self::lookup($this->data, self::parameter($parameters, 0, $name))[1] !== $value
                ? null
                : ['The :attribute field and :other must be different.', ['other' => $parameters[0]]],
            'digits' => (is_string($value) || is_int($value)) && preg_match('/^\d+$/D', (string) $value) === 1
                && strlen((string) $value) === (int) self::parameter($parameters, 0, $name)
                ? null
                : ['The :attribute field must be :digits digits.', ['digits' => $parameters[0] ?? '']],
            'min' => self::size($value, $type) >= self::number(self::parameter($parameters, 0, $name))
                ? null
                : [self::sizeMessage('min', $type), ['min' => $parameters[0]]],
            'max' => self::size($value, $type) <= self::number(self::parameter($parameters, 0, $name))
                ? null
                : [self::sizeMessage('max', $type), ['max' => $parameters[0]]],
            'between' => self::size($value, $type) >= self::number(self::parameter($parameters, 0, $name))
                && self::size($value, $type) <= self::number(self::parameter($parameters, 1, $name))
                ? null
                : [self::sizeMessage('between', $type), ['min' => $parameters[0], 'max' => $parameters[1]]],
            'size' => self::size($value, $type) == self::number(self::parameter($parameters, 0, $name))
                ? null
                : [self::sizeMessage('size', $type), ['size' => $parameters[0]]],
            'exists' => (new ExistsRule(...self::tableAndColumn($parameters, $path)))->passes($value)
                ? null
                : ['The selected :attribute is invalid.', []],
            'unique' => self::uniqueFromString($parameters, $path)->passes($value)
                ? null
                : ['The :attribute has already been taken.', []],
            default => throw new \InvalidArgumentException("Unknown validation rule {$name}."),
        };
    }

    /**
     * @param list<string> $parameters
     * @return ?array{string, array<string, string>}
     */
    private function requiredIf(array $parameters, bool $exists, mixed $value, bool $when): ?array
    {
        $other = self::parameter($parameters, 0, $when ? 'required_if' : 'required_unless');
        $otherValue = self::lookup($this->data, $other)[1];
        $values = array_slice($parameters, 1);
        $matches = self::in($otherValue, $values);
        if ($matches !== $when || self::filled($exists, $value)) {
            return null;
        }
        return $when
            ? ['The :attribute field is required when :other is :value.', ['other' => $other, 'value' => implode(', ', $values)]]
            : ['The :attribute field is required unless :other is in :values.', ['other' => $other, 'values' => implode(', ', $values)]];
    }

    /** @param list<string> $fields */
    private function anyPresent(array $fields): bool
    {
        foreach ($fields as $field) {
            [$exists, $value] = self::lookup($this->data, $field);
            if (self::filled($exists, $value)) {
                return true;
            }
        }
        return false;
    }

    /** @param list<string> $fields */
    private function allPresent(array $fields): bool
    {
        foreach ($fields as $field) {
            [$exists, $value] = self::lookup($this->data, $field);
            if (!self::filled($exists, $value)) {
                return false;
            }
        }
        return true;
    }

    /**
     * @param list<string> $parameters
     * @return ?array{string, array<string, string>}
     */
    private function compareDate(string $rule, array $parameters, mixed $value): ?array
    {
        $reference = self::parameter($parameters, 0, $rule);
        [$isField, $fieldValue] = self::lookup($this->data, $reference);
        $limit = self::toTimestamp($isField ? $fieldValue : $reference);
        $timestamp = self::toTimestamp($value);
        $passes = $limit !== null && $timestamp !== null && match ($rule) {
            'after' => $timestamp > $limit,
            'after_or_equal' => $timestamp >= $limit,
            'before' => $timestamp < $limit,
            default => $timestamp <= $limit,
        };
        if ($passes) {
            return null;
        }
        $message = match ($rule) {
            'after' => 'The :attribute field must be a date after :date.',
            'after_or_equal' => 'The :attribute field must be a date after or equal to :date.',
            'before' => 'The :attribute field must be a date before :date.',
            default => 'The :attribute field must be a date before or equal to :date.',
        };
        return [$message, ['date' => $reference]];
    }

    /** @param array<string, string> $replacements */
    private function custom(string $path, string $key, string $rule, mixed $value, array $replacements): ?string
    {
        foreach ([$path . '.' . $rule, $key . '.' . $rule, $path, $key, $rule] as $candidate) {
            if (isset($this->messages[$candidate])) {
                return $this->replace($this->messages[$candidate], $path, $value, $replacements);
            }
        }
        return null;
    }

    /** @param array<string, string> $replacements */
    private function replace(string $message, string $path, mixed $value, array $replacements): string
    {
        $replacements = [
            'attribute' => $this->attributes[$path] ?? $this->attributes[self::wildcard($path)] ?? $path,
            ...$replacements,
        ];
        $replacements['value'] ??= is_scalar($value) ? (string) $value : '';
        uksort($replacements, static fn (string $a, string $b): int => strlen($b) <=> strlen($a));
        foreach ($replacements as $name => $replacement) {
            $message = str_replace(':' . $name, $replacement, $message);
        }
        return $message;
    }

    /** @return list<string> Concrete paths of a wildcard key in the data. */
    private function expand(string $key): array
    {
        $paths = [''];
        foreach (explode('.', $key) as $segment) {
            $next = [];
            foreach ($paths as $prefix) {
                if ($segment !== '*') {
                    $next[] = $prefix === '' ? $segment : $prefix . '.' . $segment;
                    continue;
                }
                [$exists, $value] = $prefix === '' ? [true, $this->data] : self::lookup($this->data, $prefix);
                if (!$exists || !is_array($value)) {
                    continue;
                }
                foreach (array_keys($value) as $index) {
                    $next[] = $prefix === '' ? (string) $index : $prefix . '.' . $index;
                }
            }
            $paths = $next;
        }
        return $paths;
    }

    private function store(string $path, mixed $value): void
    {
        $segments = explode('.', $path);
        $target = &$this->validated;
        foreach ($segments as $position => $segment) {
            if ($position === count($segments) - 1) {
                $target[$segment] = $value;
                break;
            }
            if (!isset($target[$segment]) || !is_array($target[$segment])) {
                $target[$segment] = [];
            }
            $target = &$target[$segment];
        }
        unset($target);
    }

    /**
     * @param array<array-key, mixed> $data
     * @return array{bool, mixed} Whether the path exists, and its value.
     */
    private static function lookup(array $data, string $path): array
    {
        if (array_key_exists($path, $data)) {
            return [true, $data[$path]];
        }
        $current = $data;
        foreach (explode('.', $path) as $segment) {
            if (!is_array($current) || !array_key_exists($segment, $current)) {
                return [false, null];
            }
            $current = $current[$segment];
        }
        return [true, $current];
    }

    private static function wildcard(string $path): string
    {
        return (string) preg_replace('/\.\d+(?=\.|$)/', '.*', $path);
    }

    private static function filled(bool $exists, mixed $value): bool
    {
        return $exists && $value !== null && !(is_string($value) && trim($value) === '') && $value !== [];
    }

    /** @return array{string, list<string>} */
    private static function ruleAndParameters(string $rule): array
    {
        if (!str_contains($rule, ':')) {
            return [trim($rule), []];
        }
        [$name, $parameters] = explode(':', $rule, 2);
        $name = trim($name);
        if ($name === 'regex' || $name === 'not_regex') {
            return [$name, [$parameters]];
        }
        return [$name, array_map(static fn (?string $parameter): string => trim((string) $parameter), str_getcsv($parameters, ',', '"', ''))];
    }

    private static function ruleName(ValidationRule $rule): string
    {
        return match (true) {
            $rule instanceof ExistsRule => 'exists',
            $rule instanceof UniqueRule => 'unique',
            $rule instanceof InRule => 'in',
            $rule instanceof EnumRule => 'enum',
            default => lcfirst((new \ReflectionClass($rule))->getShortName()),
        };
    }

    /** @param list<string> $parameters */
    private static function parameter(array $parameters, int $index, string $rule): string
    {
        return $parameters[$index] ?? throw new \InvalidArgumentException("Validation rule {$rule} needs more parameters.");
    }

    private static function number(string $value): float
    {
        if (!is_numeric($value)) {
            throw new \InvalidArgumentException("Validation size {$value} is not a number.");
        }
        return (float) $value;
    }

    private static function isInteger(mixed $value): bool
    {
        return is_int($value) || (is_string($value) && preg_match('/^[+-]?\d+$/D', $value) === 1);
    }

    private static function isNumeric(mixed $value): bool
    {
        return is_int($value) || is_float($value) || (is_string($value) && is_numeric($value));
    }

    private static function toBoolean(mixed $value): ?bool
    {
        return match ($value) {
            true, 1, '1', 'true' => true,
            false, 0, '0', 'false' => false,
            default => null,
        };
    }

    /** Laravel's `date`: a parseable string with a real year, month and day. */
    private static function isDate(mixed $value): bool
    {
        if ($value instanceof \DateTimeInterface) {
            return true;
        }
        if (!is_string($value) || self::toTimestamp($value) === null) {
            return false;
        }
        $parsed = date_parse($value);
        return is_int($parsed['year']) && is_int($parsed['month']) && is_int($parsed['day'])
            && checkdate($parsed['month'], $parsed['day'], $parsed['year']);
    }

    /** A timestamp for `after`/`before`, which also take relative dates (`tomorrow`). */
    private static function toTimestamp(mixed $value): ?int
    {
        if ($value instanceof \DateTimeInterface) {
            return $value->getTimestamp();
        }
        if (!is_string($value) || $value === '') {
            return null;
        }
        $parsed = date_parse($value);
        if ($parsed['error_count'] > 0) {
            return null;
        }
        $timestamp = strtotime($value);
        return $timestamp === false ? null : $timestamp;
    }

    private static function matchesFormat(mixed $value, string $format): bool
    {
        if (!is_string($value)) {
            return false;
        }
        $date = \DateTimeImmutable::createFromFormat('!' . $format, $value);
        return $date !== false && $date->format($format) === $value;
    }

    /** @param list<string> $values */
    private static function in(mixed $value, array $values): bool
    {
        if (is_array($value)) {
            foreach ($value as $item) {
                if (!self::in($item, $values)) {
                    return false;
                }
            }
            return true;
        }
        if (is_bool($value)) {
            $value = $value ? '1' : '0';
        }
        return is_scalar($value) && in_array((string) $value, $values, true);
    }

    private static function sizeType(mixed $value, bool $numeric): string
    {
        return match (true) {
            is_array($value) => 'array',
            is_int($value), is_float($value) => 'numeric',
            $numeric && is_string($value) && is_numeric($value) => 'numeric',
            default => 'string',
        };
    }

    private static function size(mixed $value, string $type): float
    {
        return match ($type) {
            'array' => is_array($value) ? count($value) : 0,
            'numeric' => is_numeric($value) ? (float) $value : 0,
            default => is_scalar($value) ? mb_strlen((string) $value) : 0,
        };
    }

    private static function sizeMessage(string $rule, string $type): string
    {
        return match ($rule . '.' . $type) {
            'min.numeric' => 'The :attribute field must be at least :min.',
            'min.array' => 'The :attribute field must have at least :min items.',
            'min.string' => 'The :attribute field must be at least :min characters.',
            'max.numeric' => 'The :attribute field must not be greater than :max.',
            'max.array' => 'The :attribute field must not have more than :max items.',
            'max.string' => 'The :attribute field must not be greater than :max characters.',
            'between.numeric' => 'The :attribute field must be between :min and :max.',
            'between.array' => 'The :attribute field must have between :min and :max items.',
            'between.string' => 'The :attribute field must be between :min and :max characters.',
            'size.numeric' => 'The :attribute field must be :size.',
            'size.array' => 'The :attribute field must contain :size items.',
            default => 'The :attribute field must be :size characters.',
        };
    }

    /**
     * Integer, numeric and boolean strings (query and form fields) become PHP values.
     *
     * @param list<string> $names
     */
    private static function cast(array $names, mixed $value): mixed
    {
        if (!is_string($value)) {
            return in_array('boolean', $names, true) && is_int($value) ? $value === 1 : $value;
        }
        return match (true) {
            in_array('integer', $names, true) && self::isInteger($value) => (int) $value,
            in_array('numeric', $names, true) && is_numeric($value) => $value + 0,
            in_array('boolean', $names, true) && self::toBoolean($value) !== null => self::toBoolean($value),
            default => $value,
        };
    }

    /**
     * `exists:table,column` and `exists:connection.table` (column defaults to the field name).
     *
     * @param list<string> $parameters
     * @return array{string, string}
     */
    private static function tableAndColumn(array $parameters, string $path): array
    {
        $table = self::parameter($parameters, 0, 'exists');
        $column = $parameters[1] ?? null;
        if ($column === null || $column === '' || strtoupper($column) === 'NULL') {
            $segments = explode('.', $path);
            $column = end($segments);
        }
        return [$table, $column];
    }

    /**
     * `unique:table,column,except,idColumn`.
     *
     * @param list<string> $parameters
     */
    private static function uniqueFromString(array $parameters, string $path): UniqueRule
    {
        [$table, $column] = self::tableAndColumn($parameters, $path);
        $rule = new UniqueRule($table, $column);
        $except = $parameters[2] ?? null;
        if ($except !== null && $except !== '' && strtoupper($except) !== 'NULL') {
            $rule = $rule->ignore($except, $parameters[3] ?? 'id');
        }
        return $rule;
    }
}
