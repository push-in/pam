<?php

declare(strict_types=1);

namespace Nimbus\Pagination;

use Arcano\ORM\Select;
use Pam\Http\Request;

/**
 * Paginates Arcano selects from the request query: `?page=` for numbered pages and
 * `?cursor=` for keyset pages, which stay fast on large tables.
 */
final readonly class Paginator
{
    public const int MAX_PER_PAGE = 100;

    /**
     * @template T of object
     * @param Select<T> $select
     * @return Page<T>
     */
    public static function paginate(Select $select, Request $request, int $perPage = 20): Page
    {
        $perPage = self::perPage($request, $perPage);
        $page = max(1, self::integer($request->query()['page'] ?? null) ?? 1);
        // @phpstan-ignore function.alreadyNarrowedType (apps may still run Arcano 0.2.5)
        if (method_exists($select, 'fetchPage')) {
            // Arcano 0.2.6+: the page and the count in one statement when the select allows it.
            /** @var array{list<T>, int} $fetched */
            $fetched = $select->fetchPage(max(0, $perPage), max(0, ($page - 1) * $perPage));
            [$items, $total] = $fetched;
        } else {
            $total = (clone $select)->count();
            $items = (clone $select)->limit($perPage)->offset(($page - 1) * $perPage)->fetchAll();
        }
        $last = max(1, (int) ceil($total / $perPage));

        return new Page($items, [
            'current_page' => $page,
            'per_page' => $perPage,
            'last_page' => $last,
            'total' => $total,
        ], [
            'first' => self::link($request, ['page' => 1]),
            'prev' => $page > 1 ? self::link($request, ['page' => $page - 1]) : null,
            'next' => $page < $last ? self::link($request, ['page' => $page + 1]) : null,
            'last' => self::link($request, ['page' => $last]),
        ]);
    }

    /**
     * Keyset pagination ordered by a unique, sortable column (the primary key by default).
     *
     * @template T of object
     * @param Select<T> $select
     * @param non-empty-string $column
     * @return Page<T>
     */
    public static function cursor(Select $select, Request $request, int $perPage = 20, string $column = 'id', string $property = 'id'): Page
    {
        $perPage = self::perPage($request, $perPage);
        $after = self::decode($request->query()['cursor'] ?? null);
        $query = (clone $select)->orderBy($column, 'ASC')->limit($perPage + 1);
        if ($after !== null) {
            $query->where($column, '>', $after);
        }
        $items = $query->fetchAll();
        $more = count($items) > $perPage;
        if ($more) {
            array_pop($items);
        }
        $next = null;
        if ($more && $items !== []) {
            $last = $items[array_key_last($items)];
            $value = $last->{$property} ?? null;
            if (!is_int($value) && !is_string($value)) {
                throw new \LogicException("Cursor property {$property} must be an int or a string.");
            }
            $next = self::encode($value);
        }

        return new Page($items, [
            'per_page' => $perPage,
            'next_cursor' => $next,
        ], [
            'next' => $next === null ? null : self::link($request, ['cursor' => $next]),
        ]);
    }

    private static function perPage(Request $request, int $default): int
    {
        $requested = self::integer($request->query()['per_page'] ?? null) ?? $default;
        return max(1, min(self::MAX_PER_PAGE, $requested));
    }

    private static function integer(mixed $value): ?int
    {
        return is_string($value) && preg_match('/^\d{1,9}$/', $value) === 1 ? (int) $value
            : (is_int($value) ? $value : null);
    }

    /** @param array<string, int|string> $replace */
    private static function link(Request $request, array $replace): string
    {
        return $request->path . '?' . http_build_query(array_replace($request->query(), $replace));
    }

    private static function encode(int|string $value): string
    {
        return rtrim(strtr(base64_encode(json_encode($value, JSON_THROW_ON_ERROR)), '+/', '-_'), '=');
    }

    private static function decode(mixed $cursor): int|string|null
    {
        if (!is_string($cursor) || $cursor === '' || strlen($cursor) > 512) {
            return null;
        }
        $json = base64_decode(strtr($cursor, '-_', '+/'), true);
        if ($json === false) {
            return null;
        }
        try {
            $value = json_decode($json, false, 2, JSON_THROW_ON_ERROR);
        } catch (\JsonException) {
            return null;
        }
        return is_int($value) || is_string($value) ? $value : null;
    }
}
