<?php

declare(strict_types=1);

namespace Nimbus\Rector\Rector;

use PhpParser\Node;
use PhpParser\Node\Arg;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\PropertyFetch;
use PhpParser\Node\Expr\StaticCall;
use PhpParser\Node\Name\FullyQualified;
use PHPStan\Type\ObjectType;
use Rector\Rector\AbstractRector;
use Symplify\RuleDocGenerator\ValueObject\CodeSample\CodeSample;
use Symplify\RuleDocGenerator\ValueObject\RuleDefinition;

/**
 * Rewrites Laravel `Illuminate\Http\Request` calls to the PAM request API and to
 * `Nimbus\Http\RequestData` (input merged from body and query, dot notation).
 *
 * - `query('k', $d)` → `getQuery('k', $d)`, `header('k', $d)` → `getHeader('k', $d)`, `header()` → `headers()`
 * - `getContent()` → `body()`, `method()` → `->method`
 * - `input()`, `all()`, `has()`, `filled()`, `only()`, `bearerToken()` and `json('k')` → `RequestData::…($request, …)`
 */
final class LaravelRequestCallToNimbusRector extends AbstractRector
{
    private const array REQUEST_TYPES = ['Illuminate\Http\Request', 'Pam\Http\Request'];

    /** Laravel method => RequestData static method. */
    private const array REQUEST_DATA = [
        'input' => 'input',
        'all' => 'all',
        'has' => 'has',
        'filled' => 'filled',
        'only' => 'only',
        'bearerToken' => 'bearerToken',
    ];

    public function getRuleDefinition(): RuleDefinition
    {
        return new RuleDefinition('Rewrite Laravel Request calls to the PAM Request and Nimbus\Http\RequestData', [
            new CodeSample(
                <<<'CODE'
                    $name = $request->input('name', 'guest');
                    $page = $request->query('page', 1);
                    $token = $request->bearerToken();
                    CODE,
                <<<'CODE'
                    $name = \Nimbus\Http\RequestData::input($request, 'name', 'guest');
                    $page = $request->getQuery('page', 1);
                    $token = \Nimbus\Http\RequestData::bearerToken($request);
                    CODE,
            ),
        ]);
    }

    /** @return array<class-string<Node>> */
    public function getNodeTypes(): array
    {
        return [MethodCall::class];
    }

    /** @param MethodCall $node */
    public function refactor(Node $node): ?Node
    {
        if ($node->isFirstClassCallable() || !$this->isRequest($node->var)) {
            return null;
        }
        $name = $this->getName($node->name);
        $arguments = $node->getArgs();
        return match (true) {
            $name === 'query' && $arguments !== [] => $this->rename($node, 'getQuery'),
            $name === 'header' && $arguments !== [] => $this->rename($node, 'getHeader'),
            $name === 'header' => $this->rename($node, 'headers'),
            $name === 'getContent' && $arguments === [] => $this->rename($node, 'body'),
            $name === 'method' && $arguments === [] => new PropertyFetch($node->var, 'method'),
            $name === 'json' && $arguments !== [] => $this->requestData($node, 'input'),
            $name !== null && isset(self::REQUEST_DATA[$name]) => $this->requestData($node, self::REQUEST_DATA[$name]),
            default => null,
        };
    }

    private function isRequest(Node $node): bool
    {
        foreach (self::REQUEST_TYPES as $type) {
            if ($this->isObjectType($node, new ObjectType($type))) {
                return true;
            }
        }
        return false;
    }

    private function rename(MethodCall $node, string $method): MethodCall
    {
        $node->name = new Node\Identifier($method);
        return $node;
    }

    private function requestData(MethodCall $node, string $method): StaticCall
    {
        return new StaticCall(
            new FullyQualified('Nimbus\Http\RequestData'),
            $method,
            [new Arg($node->var), ...$node->getArgs()],
        );
    }
}
