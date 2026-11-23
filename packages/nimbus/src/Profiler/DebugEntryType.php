<?php

declare(strict_types=1);

namespace Nimbus\Profiler;

enum DebugEntryType: string
{
    case Request = 'request';
    case Query = 'query';
    case Job = 'job';
    case Exception = 'exception';
    case Mail = 'mail';
    case Http = 'http';
}
