<?php

declare(strict_types=1);

namespace Pam\Api\Tests\Fixtures\Arcano;

use Arcano\Annotated\Annotation\Column;
use Arcano\Annotated\Annotation\Entity;
use Arcano\Annotated\Annotation\Relation\BelongsTo;

#[Entity(role: 'post')]
class Post
{
    #[Column(type: 'primary')]
    public ?int $id = null;

    #[Column(type: 'string')]
    public string $title = '';

    #[Column(type: 'boolean', default: false)]
    public bool $published = false;

    #[BelongsTo(target: Author::class)]
    public ?Author $author = null;
}
