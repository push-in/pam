<?php

declare(strict_types=1);

namespace Nimbus\Rector\Rector;

use PhpParser\Node;
use PhpParser\Node\Arg;
use PhpParser\Node\Expr;
use PhpParser\Node\Expr\BooleanNot;
use PhpParser\Node\Expr\ClassConstFetch;
use PhpParser\Node\Expr\FuncCall;
use PhpParser\Node\Expr\New_;
use PhpParser\Node\Expr\Throw_;
use PhpParser\Node\Name\FullyQualified;
use PhpParser\Node\Scalar\Int_;
use PhpParser\Node\Scalar\String_;
use PhpParser\Node\Stmt\Expression;
use PhpParser\Node\Stmt\If_;
use Rector\Rector\AbstractRector;
use Symplify\RuleDocGenerator\ValueObject\CodeSample\CodeSample;
use Symplify\RuleDocGenerator\ValueObject\RuleDefinition;

/**
 * Rewrites `abort($status, $message)`, `abort_if()` and `abort_unless()` statements to
 * `throw new Nimbus\Http\HttpException($status, ProblemCode::…, $message)` for the
 * statuses that have a `ProblemCode`. Other statuses and non-literal statuses are left as is.
 */
final class AbortToHttpExceptionRector extends AbstractRector
{
    /** HTTP status => [ProblemCode case, default message]. */
    private const array STATUSES = [
        401 => ['Unauthenticated', 'Unauthenticated.'],
        403 => ['Forbidden', 'Forbidden.'],
        404 => ['NotFound', 'Not Found.'],
        405 => ['MethodNotAllowed', 'Method Not Allowed.'],
        408 => ['Timeout', 'Request Timeout.'],
        409 => ['Conflict', 'Conflict.'],
        413 => ['PayloadTooLarge', 'Payload Too Large.'],
        415 => ['UnsupportedMediaType', 'Unsupported Media Type.'],
        419 => ['CsrfTokenMismatch', 'CSRF token mismatch.'],
        422 => ['ValidationFailed', 'Unprocessable Content.'],
        429 => ['RateLimited', 'Too Many Requests.'],
        500 => ['InternalError', 'Server Error.'],
        504 => ['Timeout', 'Gateway Timeout.'],
    ];

    public function getRuleDefinition(): RuleDefinition
    {
        return new RuleDefinition('Rewrite abort() to throw Nimbus\Http\HttpException', [
            new CodeSample(
                <<<'CODE'
                    abort(404, 'Order not found.');
                    abort_if($order->closed, 409);
                    CODE,
                <<<'CODE'
                    throw new \Nimbus\Http\HttpException(404, \Nimbus\Http\ProblemCode::NotFound, 'Order not found.');
                    if ($order->closed) {
                        throw new \Nimbus\Http\HttpException(409, \Nimbus\Http\ProblemCode::Conflict, 'Conflict.');
                    }
                    CODE,
            ),
        ]);
    }

    /** @return array<class-string<Node>> */
    public function getNodeTypes(): array
    {
        return [Expression::class];
    }

    /** @param Expression $node */
    public function refactor(Node $node): ?Node
    {
        $call = $node->expr;
        if (!$call instanceof FuncCall || $call->isFirstClassCallable()) {
            return null;
        }
        $arguments = $call->getArgs();
        if ($this->isName($call, 'abort')) {
            $throw = $this->throw($arguments);
            return $throw === null ? null : new Expression($throw);
        }
        if (!$this->isNames($call, ['abort_if', 'abort_unless']) || count($arguments) < 2) {
            return null;
        }
        $throw = $this->throw(array_slice($arguments, 1));
        if ($throw === null) {
            return null;
        }
        $condition = $arguments[0]->value;
        if ($this->isName($call, 'abort_unless')) {
            $condition = new BooleanNot($condition);
        }
        return new If_($condition, ['stmts' => [new Expression($throw)]]);
    }

    /** @param list<Arg> $arguments `[status, message?]` */
    private function throw(array $arguments): ?Throw_
    {
        if ($arguments === [] || count($arguments) > 2 || !$arguments[0]->value instanceof Int_) {
            return null;
        }
        $status = $arguments[0]->value->value;
        if (!isset(self::STATUSES[$status])) {
            return null;
        }
        [$case, $message] = self::STATUSES[$status];
        $messageExpr = $arguments[1]->value ?? new String_($message);
        return new Throw_(new New_(new FullyQualified('Nimbus\Http\HttpException'), [
            new Arg(new Int_($status)),
            new Arg(new ClassConstFetch(new FullyQualified('Nimbus\Http\ProblemCode'), $case)),
            new Arg($messageExpr instanceof Expr ? $messageExpr : new String_($message)),
        ]));
    }
}
