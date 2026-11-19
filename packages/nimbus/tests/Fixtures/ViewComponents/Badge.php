<?php

declare(strict_types=1);

namespace Nimbus\Tests\Fixtures\ViewComponents;

use Nimbus\View\Component;

final class Badge extends Component
{
    public function __construct(public int $count, public string $color = 'gray')
    {
    }

    public function label(): string
    {
        return $this->count > 1 ? "{$this->count} items" : '1 item';
    }

    public function render(): string
    {
        return 'components.badge-view';
    }
}
