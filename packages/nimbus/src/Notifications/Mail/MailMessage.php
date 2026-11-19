<?php

declare(strict_types=1);

namespace Nimbus\Notifications\Mail;

/**
 * Fluent e-mail: greeting, lines before and after an optional action button, salutation.
 * `html()` and `text()` replace the generated bodies.
 */
final class MailMessage
{
    private string $subject = '';
    private ?string $greeting = null;
    /** @var list<string> */
    private array $introLines = [];
    /** @var list<string> */
    private array $outroLines = [];
    private ?string $actionText = null;
    private ?string $actionUrl = null;
    private ?string $salutation = null;
    private ?string $html = null;
    private ?string $text = null;
    /** @var array{address: string, name: string}|null */
    private ?array $from = null;
    /** @var list<string> */
    private array $replyTo = [];
    /** @var list<string> */
    private array $cc = [];
    /** @var list<string> */
    private array $bcc = [];

    public static function make(): self
    {
        return new self();
    }

    public function subject(string $subject): self
    {
        $this->subject = self::singleLine($subject);
        return $this;
    }

    public function greeting(string $greeting): self
    {
        $this->greeting = $greeting;
        return $this;
    }

    /** Adds a paragraph; lines added after `action()` go below the button. */
    public function line(string $line): self
    {
        if ($this->actionUrl === null) {
            $this->introLines[] = $line;
        } else {
            $this->outroLines[] = $line;
        }
        return $this;
    }

    public function action(string $text, string $url): self
    {
        if (!in_array(parse_url($url, PHP_URL_SCHEME), ['http', 'https'], true)) {
            throw new \InvalidArgumentException('Mail action URL must use http or https.');
        }
        $this->actionText = $text;
        $this->actionUrl = $url;
        return $this;
    }

    public function salutation(string $salutation): self
    {
        $this->salutation = $salutation;
        return $this;
    }

    public function html(string $html): self
    {
        $this->html = $html;
        return $this;
    }

    public function text(string $text): self
    {
        $this->text = $text;
        return $this;
    }

    public function from(string $address, string $name = ''): self
    {
        $this->from = ['address' => self::singleLine($address), 'name' => self::singleLine($name)];
        return $this;
    }

    public function replyTo(string $address): self
    {
        $this->replyTo[] = self::singleLine($address);
        return $this;
    }

    public function cc(string $address): self
    {
        $this->cc[] = self::singleLine($address);
        return $this;
    }

    public function bcc(string $address): self
    {
        $this->bcc[] = self::singleLine($address);
        return $this;
    }

    public function getSubject(): string
    {
        return $this->subject;
    }

    public function renderText(): string
    {
        if ($this->text !== null) {
            return $this->text;
        }
        $blocks = [];
        if ($this->greeting !== null) {
            $blocks[] = $this->greeting;
        }
        array_push($blocks, ...$this->introLines);
        if ($this->actionUrl !== null) {
            $blocks[] = $this->actionText . ': ' . $this->actionUrl;
        }
        array_push($blocks, ...$this->outroLines);
        if ($this->salutation !== null) {
            $blocks[] = $this->salutation;
        }
        return implode("\n\n", $blocks) . "\n";
    }

    public function renderHtml(): string
    {
        if ($this->html !== null) {
            return $this->html;
        }
        $escape = static fn (string $value): string => htmlspecialchars($value, ENT_QUOTES | ENT_SUBSTITUTE, 'UTF-8');
        $paragraph = static fn (string $line): string => '<p style="margin:0 0 16px">' . $escape($line) . '</p>';
        $body = $this->greeting === null ? '' : '<h1 style="margin:0 0 16px;font-size:20px">' . $escape($this->greeting) . '</h1>';
        $body .= implode('', array_map($paragraph, $this->introLines));
        if ($this->actionUrl !== null) {
            $body .= '<p style="margin:24px 0"><a href="' . $escape($this->actionUrl) . '" style="display:inline-block;'
                . 'padding:12px 20px;background:#1f2937;color:#ffffff;border-radius:6px;text-decoration:none">'
                . $escape((string) $this->actionText) . '</a></p>';
        }
        $body .= implode('', array_map($paragraph, $this->outroLines));
        if ($this->salutation !== null) {
            $body .= $paragraph($this->salutation);
        }
        return '<!DOCTYPE html><html><head><meta charset="utf-8"><title>' . $escape($this->subject) . '</title></head>'
            . '<body style="margin:0;padding:24px;background:#ffffff;color:#111827;font-family:Arial,sans-serif;'
            . 'font-size:15px;line-height:1.5"><div style="max-width:560px">' . $body . '</div></body></html>';
    }

    /**
     * @return array{subject: string, text: string, html: string, from: array{address: string, name: string}|null,
     *     reply_to: list<string>, cc: list<string>, bcc: list<string>}
     */
    public function toArray(): array
    {
        return [
            'subject' => $this->subject,
            'text' => $this->renderText(),
            'html' => $this->renderHtml(),
            'from' => $this->from,
            'reply_to' => $this->replyTo,
            'cc' => $this->cc,
            'bcc' => $this->bcc,
        ];
    }

    private static function singleLine(string $value): string
    {
        if (preg_match('/[\r\n\0]/', $value) === 1) {
            throw new \InvalidArgumentException('Mail header values cannot contain line breaks.');
        }
        return $value;
    }
}
