<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Nimbus\Validation\ValidationRule;

/**
 * One property of an admin resource: how it is listed, filtered, edited and validated.
 * The panel builds the defaults from the entity's Arcano columns; resources adjust them
 * fluently (`$fields['price']->label('Price')->rules(new Min(0))`) or add their own.
 *
 * Types: `id`, `text`, `email`, `url`, `password`, `textarea`, `number`, `decimal`,
 * `boolean`, `date`, `datetime`, `time`, `select`, `json` and `belongsTo`.
 */
final class AdminField
{
    public const array TYPES = [
        'id', 'text', 'email', 'url', 'password', 'textarea', 'number', 'decimal', 'boolean',
        'date', 'datetime', 'time', 'select', 'json', 'belongsTo',
    ];

    public string $label;

    public bool $required = false;

    public bool $nullable = false;

    public ?int $maxLength = null;

    /** @var list<ValidationRule> */
    public array $rules = [];

    /** @var array<int|string, string> Value => label, for `select` fields. */
    public array $options = [];

    /** @var class-string<\BackedEnum>|null Backed enum the select maps to. */
    public ?string $enum = null;

    /** @var class-string|null Entity of a `belongsTo` field. */
    public ?string $target = null;

    public string $help = '';

    public bool $readonly = false;

    public bool $onList;

    public bool $onForm;

    public bool $sortable;

    public bool $searchable;

    public bool $filterable;

    /** @var (\Closure(mixed, object): string)|null */
    public ?\Closure $format = null;

    public function __construct(public readonly string $name, public readonly string $type = 'text')
    {
        if ($name === '' || !in_array($type, self::TYPES, true)) {
            throw new \InvalidArgumentException("Invalid admin field {$name} of type {$type}.");
        }
        $this->label = self::headline($name);
        $this->onList = !in_array($type, ['textarea', 'json', 'password'], true);
        $this->onForm = $type !== 'id';
        $this->sortable = !in_array($type, ['textarea', 'json', 'password', 'belongsTo'], true);
        $this->searchable = in_array($type, ['text', 'email', 'url'], true);
        $this->filterable = in_array($type, ['boolean', 'select', 'belongsTo'], true);
        if ($type === 'id') {
            $this->readonly = true;
        }
    }

    public static function text(string $name): self
    {
        return new self($name, 'text');
    }

    public static function textarea(string $name): self
    {
        return new self($name, 'textarea');
    }

    public static function email(string $name): self
    {
        return new self($name, 'email');
    }

    public static function number(string $name): self
    {
        return new self($name, 'number');
    }

    public static function decimal(string $name): self
    {
        return new self($name, 'decimal');
    }

    public static function boolean(string $name): self
    {
        return new self($name, 'boolean');
    }

    public static function date(string $name): self
    {
        return new self($name, 'date');
    }

    public static function datetime(string $name): self
    {
        return new self($name, 'datetime');
    }

    /** @param array<int|string, string>|string $options Value => label, or a backed enum class. */
    public static function select(string $name, array|string $options): self
    {
        return (new self($name, 'select'))->options($options);
    }

    /** @param class-string $target The related entity; its options come from its admin resource title. */
    public static function belongsTo(string $name, string $target): self
    {
        $field = new self($name, 'belongsTo');
        $field->target = $target;
        return $field;
    }

    public function label(string $label): self
    {
        $this->label = $label;
        return $this;
    }

    public function required(bool $required = true): self
    {
        $this->required = $required;
        if ($required) {
            $this->nullable = false;
        }
        return $this;
    }

    public function nullable(bool $nullable = true): self
    {
        $this->nullable = $nullable;
        if ($nullable) {
            $this->required = false;
        }
        return $this;
    }

    public function maxLength(?int $length): self
    {
        $this->maxLength = $length;
        return $this;
    }

    /** Extra rules, checked after the value is cast (`new Min(0)`, `new Email()`...). */
    public function rules(ValidationRule ...$rules): self
    {
        $this->rules = [...$this->rules, ...array_values($rules)];
        return $this;
    }

    /** @param array<int|string, string>|string $options Value => label, or a backed enum class. */
    public function options(array|string $options): self
    {
        if (is_string($options)) {
            if (!is_a($options, \BackedEnum::class, true)) {
                throw new \InvalidArgumentException("{$options} is not a backed enum.");
            }
            $this->enum = $options;
            $labels = [];
            foreach ($options::cases() as $case) {
                $labels[$case->value] = self::headline($case->name);
            }
            $options = $labels;
        }
        $this->options = $options;
        return $this;
    }

    public function help(string $help): self
    {
        $this->help = $help;
        return $this;
    }

    /** Shown on the form but never written. */
    public function readonly(bool $readonly = true): self
    {
        $this->readonly = $readonly;
        return $this;
    }

    public function onList(bool $show = true): self
    {
        $this->onList = $show;
        return $this;
    }

    public function onForm(bool $show = true): self
    {
        $this->onForm = $show;
        return $this;
    }

    public function hidden(): self
    {
        $this->onList = false;
        $this->onForm = false;
        $this->searchable = false;
        $this->filterable = false;
        return $this;
    }

    public function sortable(bool $sortable = true): self
    {
        $this->sortable = $sortable;
        return $this;
    }

    public function searchable(bool $searchable = true): self
    {
        $this->searchable = $searchable;
        return $this;
    }

    public function filterable(bool $filterable = true): self
    {
        $this->filterable = $filterable;
        return $this;
    }

    /** @param \Closure(mixed, object): string $format Text shown in the list for the value. */
    public function format(\Closure $format): self
    {
        $this->format = $format;
        return $this;
    }

    /** `created_at` => `Created at`, `authorId` => `Author id`. */
    public static function headline(string $name): string
    {
        $words = strtolower(trim((string) preg_replace(['/([a-z0-9])([A-Z])/', '/[_\-\s]+/'], ['$1 $2', ' '], $name)));
        return ucfirst($words);
    }
}
