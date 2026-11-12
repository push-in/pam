<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Fixtures\Arcano;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;
use Arcano\Annotated\Annotation\Relation\HasMany;

#[Entity(role: 'author')]
class Author
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string(120)')]
    public string $name = '';

    /** @var list<Post> */
    #[HasMany(target: Post::class, collection: 'array')]
    public array $posts = [];
}
