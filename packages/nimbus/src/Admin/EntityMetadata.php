<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Arcano\Annotated\Annotation\Column;
use Arcano\ORM\Relation;
use Arcano\ORM\SchemaInterface;
use Nimbus\Validation\ValidationRule;

/**
 * @internal What the admin knows about an entity: its role, primary key and the default
 * fields read from the compiled Arcano schema and the `#[Column]` attributes.
 */
final readonly class EntityMetadata
{
    private const array HIDDEN = ['password', 'password_hash', 'remember_token', 'secret', 'two_factor_secret'];

    /**
     * @param class-string $class
     * @param non-empty-string $primaryKey
     * @param array<string, AdminField> $fields
     * @param array<string, string> $innerKeys Relation name => inner key field of belongs-to relations.
     */
    public function __construct(
        public string $role,
        public string $class,
        public string $primaryKey,
        public array $fields,
        public array $innerKeys,
    ) {
    }

    /** @param class-string $class */
    public static function of(SchemaInterface $schema, string $class): self
    {
        $role = $schema->resolveAlias($class) ?? $class;
        if (!$schema->defines($role)) {
            throw new \InvalidArgumentException("{$class} is not an Arcano entity of the application schema.");
        }
        $primary = $schema->define($role, SchemaInterface::PRIMARY_KEY);
        $first = is_array($primary) ? reset($primary) : $primary;
        $primary = is_string($first) && $first !== '' ? $first : 'id';
        $columns = $schema->define($role, SchemaInterface::COLUMNS);
        $columns = is_array($columns) ? $columns : [];
        $typecast = $schema->define($role, SchemaInterface::TYPECAST);
        $typecast = is_array($typecast) ? $typecast : [];
        $reflection = new \ReflectionClass($class);

        $innerKeys = [];
        $relations = [];
        foreach ($schema->getRelations($role) as $name) {
            $relation = $schema->defineRelation($role, $name);
            if (($relation[Relation::TYPE] ?? null) !== Relation::BELONGS_TO) {
                continue;
            }
            $targetRole = $relation[Relation::TARGET] ?? null;
            $target = is_string($targetRole) ? $schema->define($targetRole, SchemaInterface::ENTITY) : null;
            $options = is_array($relation[Relation::SCHEMA] ?? null) ? $relation[Relation::SCHEMA] : [];
            $inner = $options[Relation::INNER_KEY] ?? null;
            $inner = is_array($inner) ? reset($inner) : $inner;
            if (!is_string($target) || !class_exists($target) || !is_string($inner)) {
                continue;
            }
            $innerKeys[$name] = $inner;
            $field = AdminField::belongsTo($name, $target);
            $field->nullable((bool) ($options[Relation::NULLABLE] ?? false));
            $field->required(!$field->nullable);
            $relations[$name] = $field;
        }

        $fields = [];
        foreach (array_keys($columns) as $property) {
            $property = (string) $property;
            if (in_array($property, $innerKeys, true) || !$reflection->hasProperty($property)) {
                continue;
            }
            $fields[$property] = self::field(
                $reflection->getProperty($property),
                $property === $primary,
                is_string($typecast[$property] ?? null) ? $typecast[$property] : null,
            );
        }
        // Relations follow the scalar columns, in declaration order.
        return new self($role, $class, $primary, $fields + $relations, $innerKeys);
    }

    private static function field(\ReflectionProperty $property, bool $primary, ?string $typecast): AdminField
    {
        $name = $property->getName();
        $attribute = $property->getAttributes(Column::class)[0] ?? null;
        $column = $attribute?->newInstance();
        $type = strtolower($column?->getType() ?? ($typecast ?? 'string'));
        $base = (string) preg_replace('/\(.*$/', '', $type);
        $arguments = preg_match('/\((.*)\)/', $type, $match) === 1
            ? array_map(static fn (string $part): string => trim($part, " '\""), explode(',', $match[1]))
            : [];
        $phpType = $property->getType();
        $phpClass = $phpType instanceof \ReflectionNamedType && !$phpType->isBuiltin() ? $phpType->getName() : null;

        $field = match (true) {
            $primary || in_array($base, ['primary', 'bigprimary', 'smallprimary'], true) => new AdminField($name, 'id'),
            $phpClass !== null && is_a($phpClass, \BackedEnum::class, true) => AdminField::select($name, $phpClass),
            $base === 'enum' => AdminField::select($name, array_combine($arguments, array_map(AdminField::headline(...), $arguments))),
            in_array($base, ['text', 'tinytext', 'mediumtext', 'longtext'], true) => new AdminField($name, 'textarea'),
            in_array($base, ['json', 'jsonb'], true) => new AdminField($name, 'json'),
            in_array($base, ['integer', 'int', 'tinyinteger', 'smallinteger', 'biginteger', 'bigint'], true) => new AdminField($name, 'number'),
            in_array($base, ['float', 'double', 'decimal', 'real'], true) => new AdminField($name, 'decimal'),
            in_array($base, ['boolean', 'bool'], true) => new AdminField($name, 'boolean'),
            $base === 'date' => new AdminField($name, 'date'),
            in_array($base, ['datetime', 'timestamp', 'datetimetz', 'timestamptz'], true) => new AdminField($name, 'datetime'),
            $base === 'time' => new AdminField($name, 'time'),
            in_array($name, self::HIDDEN, true) || str_starts_with($name, 'password') => new AdminField($name, 'password'),
            $name === 'email' || str_ends_with($name, '_email') => new AdminField($name, 'email'),
            $name === 'url' || str_ends_with($name, '_url') => new AdminField($name, 'url'),
            default => new AdminField($name, 'text'),
        };
        if ($field->type === 'password') {
            $field->hidden();
        }
        if (in_array($field->type, ['text', 'email', 'url'], true)) {
            $size = in_array($base, ['string', 'char'], true) ? (int) ($arguments[0] ?? 255) : null;
            $field->maxLength($size !== null && $size > 0 ? $size : null);
        }
        $nullable = $column?->isNullable() ?? ($phpType === null || $phpType->allowsNull());
        $hasDefault = ($column?->hasDefault() ?? false) || $field->type === 'boolean';
        $field->nullable = $nullable;
        $field->required = !$nullable && !$hasDefault && $field->type !== 'id';
        foreach ($property->getAttributes() as $rule) {
            if (is_a($rule->getName(), ValidationRule::class, true)) {
                $instance = $rule->newInstance();
                if ($instance instanceof ValidationRule) {
                    $field->rules($instance);
                }
            }
        }
        return $field;
    }
}
