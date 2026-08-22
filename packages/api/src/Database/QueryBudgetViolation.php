<?php

declare(strict_types=1);

namespace Pam\Api\Database;

enum QueryBudgetViolation: int
{
    case QueryCount = 1;
    case ElapsedTime = 2;
    case DuplicateQuery = 3;
}
