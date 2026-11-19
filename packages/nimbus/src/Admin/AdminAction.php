<?php

declare(strict_types=1);

namespace Nimbus\Admin;

/**
 * A bulk action of a resource list: runs over the selected entities after each one passes
 * the Gate check of `$ability` (`update` by default). The handler receives the entities and
 * the Arcano manager and may return the flash message shown afterwards.
 *
 * ```php
 * public function actions(): array
 * {
 *     return ['publish' => new AdminAction('Publish', static function (array $posts, ArcanoManager $arcano): string {
 *         $manager = $arcano->entityManager();
 *         foreach ($posts as $post) { $post->published = true; $manager->persist($post); }
 *         $manager->run();
 *         return count($posts) . ' published.';
 *     })];
 * }
 * ```
 */
final readonly class AdminAction
{
    /** @param \Closure(list<object>, \Nimbus\Database\ArcanoManager): ?string $handler */
    public function __construct(
        public string $label,
        public \Closure $handler,
        public string $ability = 'update',
        public bool $destructive = false,
    ) {
        if ($label === '' || $ability === '') {
            throw new \InvalidArgumentException('An admin action needs a label and an ability.');
        }
    }
}
