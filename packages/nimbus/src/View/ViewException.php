<?php

declare(strict_types=1);

namespace Nimbus\View;

/** A template that failed to compile or render, with its file and line. */
final class ViewException extends \RuntimeException
{
    public function __construct(
        string $message,
        public readonly string $template,
        public readonly int $templateLine,
        ?\Throwable $previous = null,
    ) {
        parent::__construct("{$message} ({$template}:{$templateLine})", 0, $previous);
    }

    /** The message without the location. */
    public function reason(): string
    {
        $suffix = " ({$this->template}:{$this->templateLine})";
        $message = $this->getMessage();
        return str_ends_with($message, $suffix) ? substr($message, 0, -strlen($suffix)) : $message;
    }
}
