<?php

declare(strict_types=1);

namespace Nimbus\Admin;

use Arcano\ORM\Select;
use Nimbus\Validation\Input;
use Pam\Http\Request;

/**
 * An Arcano entity in the admin panel. Without a subclass the panel lists and edits every
 * column; a subclass narrows or relabels fields, scopes the query, adds bulk actions or
 * validates through an `Input` class.
 *
 * ```php
 * final class ProductAdmin extends AdminResource
 * {
 *     protected string $entity = Product::class;
 *
 *     public function fields(array $fields): array
 *     {
 *         $fields['price']->label('Price (cents)')->rules(new Min(0));
 *         unset($fields['internal_notes']);
 *         return $fields;
 *     }
 * }
 * ```
 *
 * Authorization: every route needs the `viewAdmin` ability; when the entity has a Gate policy
 * (or the Gate defines the ability), the list needs `viewAny`, the form `create` or `update`
 * and removal `delete`, checked against the entity.
 */
class AdminResource
{
    /** @var class-string */
    protected string $entity;

    /** @param class-string|null $entity */
    public function __construct(?string $entity = null)
    {
        if ($entity !== null) {
            $this->entity = $entity;
        }
        if (!isset($this->entity) || !class_exists($this->entity)) {
            throw new \LogicException(static::class . ' must name an entity class.');
        }
    }

    /** @return class-string */
    public function entity(): string
    {
        return $this->entity;
    }

    /** URL segment: `BlogPost` => `blog-posts`. */
    public function slug(): string
    {
        $short = substr($this->entity, (int) strrpos('\\' . $this->entity, '\\'));
        $kebab = strtolower((string) preg_replace('/([a-z0-9])([A-Z])/', '$1-$2', $short));
        return self::plural($kebab);
    }

    public function label(): string
    {
        return AdminField::headline($this->slug());
    }

    public function singularLabel(): string
    {
        return AdminField::headline(substr($this->entity, (int) strrpos('\\' . $this->entity, '\\')));
    }

    /**
     * The fields of the list and the form, keyed by property. Receives the defaults read from
     * the entity; return them changed, reordered or replaced.
     *
     * @param array<string, AdminField> $fields
     * @return array<string, AdminField>
     */
    public function fields(array $fields): array
    {
        return $fields;
    }

    public function perPage(): int
    {
        return 25;
    }

    /** @return array{string, 'asc'|'desc'}|null Field and direction; null sorts by primary key, newest first. */
    public function defaultSort(): ?array
    {
        return null;
    }

    /** How an entity is named in titles, flash messages and belongs-to selects. */
    public function title(object $entity): string
    {
        foreach (['name', 'title', 'label', 'email', 'slug'] as $property) {
            if (isset($entity->{$property}) && is_scalar($entity->{$property}) && (string) $entity->{$property} !== '') {
                return (string) $entity->{$property};
            }
        }
        foreach (['id', 'uuid'] as $property) {
            if (isset($entity->{$property}) && is_scalar($entity->{$property})) {
                return $this->singularLabel() . ' #' . $entity->{$property};
            }
        }
        return $this->singularLabel();
    }

    /**
     * An Input class validating the form instead of the field rules. Its public properties
     * with the field names are copied to the entity.
     *
     * @return class-string<Input>|null
     */
    public function input(): ?string
    {
        return null;
    }

    /** @return array<string, AdminAction> Bulk actions by name, besides `delete`. */
    public function actions(): array
    {
        return [];
    }

    /**
     * Scopes the list (and the entities the panel loads by id), e.g. to the current tenant.
     *
     * @template T of object
     * @param Select<T> $select
     * @return Select<T>
     */
    public function query(Select $select, Request $request): Select
    {
        return $select;
    }

    /** Runs before the entity is persisted, after the fields were written to it. */
    public function saving(object $entity, bool $creating): void
    {
    }

    private static function plural(string $word): string
    {
        return match (true) {
            preg_match('/[^aeiou]y$/', $word) === 1 => substr($word, 0, -1) . 'ies',
            preg_match('/(s|x|z|ch|sh)$/', $word) === 1 => $word . 'es',
            default => $word . 's',
        };
    }
}
