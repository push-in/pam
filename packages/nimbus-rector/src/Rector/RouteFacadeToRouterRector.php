<?php

declare(strict_types=1);

namespace Nimbus\Rector\Rector;

use PhpParser\Node;
use PhpParser\Node\Expr\ArrowFunction;
use PhpParser\Node\Expr\Closure;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\StaticCall;
use PhpParser\Node\Expr\Variable;
use PhpParser\Node\Name\FullyQualified;
use PhpParser\Node\Param;
use PhpParser\Node\Scalar\String_;
use Rector\Rector\AbstractRector;
use Symplify\RuleDocGenerator\ValueObject\CodeSample\CodeSample;
use Symplify\RuleDocGenerator\ValueObject\RuleDefinition;

/**
 * Rewrites `Route::…` facade calls of a Laravel routes file to the `$router`
 * (`Nimbus\RouteRegistrar`) that Nimbus passes to `routes.php`:
 *
 * - `Route::get|post|put|patch|delete|options|prefix|middleware|resource|apiResource(…)` → `$router->…(…)`
 * - `->middleware('auth')`, `'auth:sanctum'` or `'auth:api'` → `->auth()`
 * - group closures without parameters receive `RouteRegistrar $router`
 *
 * Laravel's array-style `Route::group([...], fn)` and `Route::controller()->group()` are left as is.
 */
final class RouteFacadeToRouterRector extends AbstractRector
{
    private const array FACADES = ['Illuminate\Support\Facades\Route', 'Route'];

    private const array METHODS = [
        'get', 'post', 'put', 'patch', 'delete', 'options', 'head',
        'prefix', 'middleware', 'resource', 'apiResource',
    ];

    private const array AUTH_MIDDLEWARE = ['auth', 'auth:sanctum', 'auth:api'];

    public function getRuleDefinition(): RuleDefinition
    {
        return new RuleDefinition('Rewrite Laravel Route facade calls to the Nimbus $router', [
            new CodeSample(
                <<<'CODE'
                    Route::middleware('auth:sanctum')->prefix('v1')->group(function () {
                        Route::get('orders/{id}', [OrderController::class, 'show'])->name('orders.show');
                    });
                    CODE,
                <<<'CODE'
                    $router->auth()->prefix('v1')->group(function (\Nimbus\RouteRegistrar $router) {
                        $router->get('orders/{id}', [OrderController::class, 'show'])->name('orders.show');
                    });
                    CODE,
            ),
        ]);
    }

    /** @return array<class-string<Node>> */
    public function getNodeTypes(): array
    {
        return [StaticCall::class, MethodCall::class];
    }

    /** @param StaticCall|MethodCall $node */
    public function refactor(Node $node): ?Node
    {
        if ($node instanceof StaticCall) {
            if (!$this->isNames($node->class, self::FACADES) || !$this->isNames($node->name, self::METHODS)) {
                return null;
            }
            $call = new MethodCall(new Variable('router'), $node->name, $node->args);
            return $this->authMiddleware($call) ?? $call;
        }
        if (!$this->isRouterChain($node)) {
            return null;
        }
        if ($this->isName($node->name, 'middleware')) {
            return $this->authMiddleware($node);
        }
        if ($this->isName($node->name, 'group') && count($node->args) === 1) {
            $routes = $node->getArgs()[0]->value;
            if (($routes instanceof Closure || $routes instanceof ArrowFunction) && $routes->params === []) {
                $routes->params[] = new Param(new Variable('router'), type: new FullyQualified('Nimbus\RouteRegistrar'));
                return $node;
            }
        }
        return null;
    }

    private function authMiddleware(MethodCall $call): ?MethodCall
    {
        if (!$this->isName($call->name, 'middleware') || count($call->args) !== 1) {
            return null;
        }
        $middleware = $call->getArgs()[0]->value;
        if (!$middleware instanceof String_ || !in_array($middleware->value, self::AUTH_MIDDLEWARE, true)) {
            return null;
        }
        return new MethodCall($call->var, 'auth');
    }

    /** Whether the call chain starts at `$router` or at a `Route::` facade call. */
    private function isRouterChain(MethodCall $call): bool
    {
        $root = $call->var;
        while ($root instanceof MethodCall) {
            $root = $root->var;
        }
        if ($root instanceof Variable) {
            return $this->isName($root, 'router');
        }
        return $root instanceof StaticCall && $this->isNames($root->class, self::FACADES);
    }
}
