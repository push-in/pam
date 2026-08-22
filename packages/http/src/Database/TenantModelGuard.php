<?php

declare(strict_types=1);

namespace Pam\Api\Database;

use Illuminate\Database\Eloquent\Model;
use Pam\Api\Container\Container;

final readonly class TenantModelGuard
{
    public function __construct(private Container $container)
    {
    }

    /** @param class-string<Model> $model */
    public function protect(string $model, string $column = 'tenant_id'): void
    {
        if (!is_subclass_of($model, Model::class)) {
            throw new \InvalidArgumentException("{$model} must extend Eloquent Model.");
        }
        $scope = new TenantScope($this->container, $column);
        $model::addGlobalScope($scope);
        $model::creating(static function (Model $instance) use ($scope): void {
            $scope->assign($instance);
        });
    }
}
