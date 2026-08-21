<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Database\Eloquent\Builder;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Database\Eloquent\Scope;
use Pam\Api\Container\Container;
use Pam\Api\Tenancy\TenantContext;

/** @implements Scope<Model> */
final readonly class TenantScope implements Scope
{
    public function __construct(
        private Container $container,
        private string $column = 'tenant_id',
    ) {
        if ($column === '') {
            throw new \InvalidArgumentException('The tenant column cannot be empty.');
        }
    }

    public function apply(Builder $builder, Model $model): void
    {
        $context = $this->context();
        $builder->where($model->qualifyColumn($this->column), $context->tenant->identifier());
    }

    public function assign(Model $model): void
    {
        $context = $this->context();
        $current = $model->getAttribute($this->column);
        if ($current !== null && !is_string($current) && !is_int($current) && !$current instanceof \Stringable) {
            throw new \LogicException('The existing tenant key must be string-compatible.');
        }
        if ($current !== null && (string) $current !== $context->tenant->identifier()) {
            throw new \LogicException('A tenant-protected model cannot be assigned to another tenant.');
        }
        $model->setAttribute($this->column, $context->tenant->identifier());
    }

    private function context(): TenantContext
    {
        $context = $this->container->scopedValue(TenantContext::class);
        if (!$context instanceof TenantContext) {
            throw new \LogicException('A tenant-protected Eloquent operation requires an active TenantContext.');
        }
        return $context;
    }
}
