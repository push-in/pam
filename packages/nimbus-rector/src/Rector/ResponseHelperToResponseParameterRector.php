<?php

declare(strict_types=1);

namespace Nimbus\Rector\Rector;

use PhpParser\Node;
use PhpParser\Node\Arg;
use PhpParser\Node\Expr\FuncCall;
use PhpParser\Node\Expr\MethodCall;
use PhpParser\Node\Expr\Variable;
use PhpParser\Node\Name\FullyQualified;
use PhpParser\Node\Param;
use PhpParser\Node\Scalar\Int_;
use PhpParser\Node\Stmt\ClassMethod;
use PhpParser\NodeVisitor;
use Rector\Rector\AbstractRector;
use Symplify\RuleDocGenerator\ValueObject\CodeSample\CodeSample;
use Symplify\RuleDocGenerator\ValueObject\RuleDefinition;

/**
 * Rewrites the Laravel `response()` helper inside a controller method to the
 * `Pam\Http\Response` the action receives, adding a `Response $response` parameter
 * when the method has none:
 *
 * - `response()->json($data, $status)` → `$response->json($data, $status)`
 * - `response()->noContent()` → `$response->status(204)`
 *
 * Calls with headers or JSON options (a third argument) and Laravel middleware
 * (`handle($request, Closure $next)`, ported by hand to Nimbus middleware) are left for manual review.
 */
final class ResponseHelperToResponseParameterRector extends AbstractRector
{
    private const string RESPONSE = 'Pam\Http\Response';

    public function getRuleDefinition(): RuleDefinition
    {
        return new RuleDefinition('Rewrite response()->json() to the injected Pam\Http\Response', [
            new CodeSample(
                <<<'CODE'
                    public function show(Request $request)
                    {
                        return response()->json(['id' => 1], 201);
                    }
                    CODE,
                <<<'CODE'
                    public function show(Request $request, \Pam\Http\Response $response)
                    {
                        return $response->json(['id' => 1], 201);
                    }
                    CODE,
            ),
        ]);
    }

    /** @return array<class-string<Node>> */
    public function getNodeTypes(): array
    {
        return [ClassMethod::class];
    }

    /** @param ClassMethod $node */
    public function refactor(Node $node): ?Node
    {
        if ($node->stmts === null || $this->isLaravelMiddleware($node)) {
            return null;
        }
        $variable = $this->responseVariable($node);
        $changed = false;
        $this->traverseNodesWithCallable($node->stmts, function (Node $inner) use ($variable, &$changed): int|Node|null {
            if ($inner instanceof Node\Expr\Closure || $inner instanceof Node\Stmt\Class_) {
                return NodeVisitor::DONT_TRAVERSE_CURRENT_AND_CHILDREN;
            }
            if (!$inner instanceof MethodCall || !$this->isResponseHelper($inner->var)) {
                return null;
            }
            $arguments = $inner->getArgs();
            if ($this->isName($inner->name, 'json') && count($arguments) <= 2) {
                $changed = true;
                return new MethodCall(new Variable($variable), 'json', $arguments === [] ? [new Arg(new Node\Expr\ConstFetch(new Node\Name('null')))] : $arguments);
            }
            if ($this->isName($inner->name, 'noContent') && $arguments === []) {
                $changed = true;
                return new MethodCall(new Variable($variable), 'status', [new Arg(new Int_(204))]);
            }
            return null;
        });
        if (!$changed) {
            return null;
        }
        if (!$this->hasParameter($node, $variable)) {
            $node->params[] = new Param(new Variable($variable), type: new FullyQualified(self::RESPONSE));
        }
        return $node;
    }

    private function isLaravelMiddleware(ClassMethod $method): bool
    {
        foreach ($method->params as $param) {
            if ($param->type !== null && $this->isName($param->type, 'Closure')) {
                return true;
            }
        }
        return false;
    }

    private function isResponseHelper(Node $node): bool
    {
        return $node instanceof FuncCall && $this->isName($node, 'response') && $node->args === [];
    }

    /** The name of an existing `Pam\Http\Response` parameter, else `response`. */
    private function responseVariable(ClassMethod $method): string
    {
        foreach ($method->params as $param) {
            if ($param->type !== null && $this->isNames($param->type, [self::RESPONSE, 'Illuminate\Http\Response'])) {
                $name = $this->getName($param->var);
                if ($name !== null) {
                    return $name;
                }
            }
        }
        return 'response';
    }

    private function hasParameter(ClassMethod $method, string $name): bool
    {
        foreach ($method->params as $param) {
            if ($this->isName($param->var, $name)) {
                return true;
            }
        }
        return false;
    }
}
