<?php

declare(strict_types=1);

namespace Nimbus\Tests\PHPStan;

use Nimbus\PHPStan\RouteParameterRule;
use PHPStan\Rules\Rule;
use PHPStan\Testing\RuleTestCase;
use PHPUnit\Framework\Attributes\CoversClass;

/** @extends RuleTestCase<RouteParameterRule> */
#[CoversClass(RouteParameterRule::class)]
final class RouteParameterRuleTest extends RuleTestCase
{
    protected function getRule(): Rule
    {
        return new RouteParameterRule();
    }

    public function testScalarParametersNeedStringPlaceholders(): void
    {
        $this->analyse([__DIR__ . '/data/routes.php'], [
            ['Route /orders/{order}/items has no {id} placeholder for the closure parameter $id.', 13],
            ['Route /teams/{team} has no {page} placeholder for the closure parameter $page.', 14],
            ['Route parameter {number} of /pages/{number} is a string; declare $number as string and convert it.', 16],
        ]);
    }
}
