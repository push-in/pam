<?php

declare(strict_types=1);

namespace Nimbus\Tests\PHPStan\data;

use Nimbus\Container\Container;
use Nimbus\Database\ArcanoManager;
use Nimbus\Tests\Fixtures\Arcano\Author;
use Nimbus\Tests\PHPStan\Fixtures\StoreOrderRequest;

use function PHPStan\Testing\assertType;

function container(Container $container, string $id): void
{
    assertType(ArcanoManager::class, $container->get(ArcanoManager::class));
    assertType('mixed', $container->get('config.value'));
    assertType('mixed', $container->get($id));
}

function arcano(ArcanoManager $arcano, string $role): void
{
    assertType('Arcano\ORM\RepositoryInterface<' . Author::class . '>', $arcano->repository(Author::class));
    assertType('Arcano\ORM\Select<' . Author::class . '>', $arcano->select(Author::class));
    assertType('Arcano\ORM\Select<object>', $arcano->select($role));
}

function input(StoreOrderRequest $request): void
{
    assertType('non-empty-string', $request->input('sku'));
    assertType('int', $request->input('quantity'));
    assertType('bool|null', $request->input('gift'));
    assertType('bool', $request->input('gift', false));
    assertType('string|null', $request->input('note'));
    assertType('array|null', $request->input('tags'));
    assertType('mixed', $request->input('status'));
    assertType('mixed', $request->input('unknown'));
    assertType('array{sku: non-empty-string, quantity: int, gift?: bool, note?: string, tags?: array, status: mixed}', $request->validated());
}
