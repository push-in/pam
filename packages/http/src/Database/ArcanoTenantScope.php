<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Arcano\ORM\Heap\State;
use Arcano\ORM\Select\QueryBuilder;
use Arcano\ORM\Select\ScopeInterface;
use Pam\Api\Container\Container;
use Pam\Api\Tenancy\TenantContext;

/**
 * Arcano scope of a tenant-protected role, see {@see ArcanoManager::protectTenant()}.
 */
final readonly class ArcanoTenantScope implements ScopeInterface
{
    public function __construct(
        private Container $container,
        private string $column = 'tenant_id',
    ) {
        if ($column === '') {
            throw new \InvalidArgumentException('The tenant column cannot be empty.');
        }
    }

    public function apply(QueryBuilder $query): void
    {
        $query->where($this->column, $this->context()->tenant->identifier());
    }

    /**
     * Stamps the current tenant on the data of a new entity.
     */
    public function assign(State $state): void
    {
        $tenant = $this->context()->tenant->identifier();
        $current = $state->getData()[$this->column] ?? null;
        if ($current !== null && !is_string($current) && !is_int($current) && !$current instanceof \Stringable) {
            throw new \LogicException('The existing tenant key must be string-compatible.');
        }
        if ($current !== null && (string) $current !== $tenant) {
            throw new \LogicException('A tenant-protected entity cannot be assigned to another tenant.');
        }
        $state->register($this->column, $tenant);
    }

    private function context(): TenantContext
    {
        $context = $this->container->scopedValue(TenantContext::class);
        if (!$context instanceof TenantContext) {
            throw new \LogicException('A tenant-protected Arcano operation requires an active TenantContext.');
        }
        return $context;
    }
}
