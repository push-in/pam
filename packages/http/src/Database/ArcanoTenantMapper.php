<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\ORM\Command\CommandInterface;
use Arcano\ORM\Heap\Node;
use Arcano\ORM\Heap\State;
use Arcano\ORM\Mapper\Mapper;

/**
 * Mapper of tenant-protected roles: new entities get the current tenant.
 */
class ArcanoTenantMapper extends Mapper
{
    public function queueCreate(object $entity, Node $node, State $state): CommandInterface
    {
        $scope = $this->source->getScope();
        if ($scope instanceof ArcanoTenantScope) {
            $scope->assign($state);
        }
        return parent::queueCreate($entity, $node, $state);
    }
}
