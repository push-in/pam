<?php

declare(strict_types=1);

namespace Pam\Api\Middleware;

use Pam\Api\Database\QueryBudget;
use Pam\Api\Database\QueryBudgetExceededException;
use Pam\Api\Database\QueryMonitor;
use Pam\Contracts\Http\MiddlewareInterface;
use Pam\Contracts\Http\RequestHandlerInterface;
use Pam\Http\Request;
use Pam\Http\Response;

final readonly class QueryBudgetMiddleware implements MiddlewareInterface
{
    public function __construct(
        private QueryMonitor $monitor,
        private QueryBudget $budget = new QueryBudget(),
        private bool $failOnViolation = false,
    ) {
    }

    public function process(Request $request, Response $response, RequestHandlerInterface $next): Response
    {
        $this->monitor->begin($this->budget);
        try {
            $result = $next->handle($request, $response);
        } catch (\Throwable $error) {
            $this->monitor->finish();
            throw $error;
        }
        $report = $this->monitor->finish();
        if ($this->failOnViolation && !$report->withinBudget()) {
            throw new QueryBudgetExceededException($report);
        }
        return $result;
    }
}
