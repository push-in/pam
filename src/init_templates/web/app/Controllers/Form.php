<?php

declare(strict_types=1);

namespace App\Controllers;

use Nimbus\Http\FormData;
use Nimbus\Http\Redirect;
use Nimbus\Session\Session;
use Pam\Http\Request;

/** Reads a submitted form and sends it back with its errors, keeping what the user typed. */
final class Form
{
    /** @param array<string, string> $fields */
    private function __construct(private array $fields)
    {
    }

    public static function from(Request $request): self
    {
        $fields = [];
        foreach (FormData::parse($request) as $name => $value) {
            if (is_string($name) && is_string($value)) {
                $fields[$name] = $value;
            }
        }
        return new self($fields);
    }

    public function string(string $name): string
    {
        return trim($this->fields[$name] ?? '');
    }

    /** Passwords are never trimmed. */
    public function secret(string $name): string
    {
        return $this->fields[$name] ?? '';
    }

    /**
     * @param array<string, list<string>> $errors
     * @param list<string> $keep Fields flashed back as old input (never passwords).
     */
    public static function back(Session $session, string $to, array $errors, self $form, array $keep = []): Redirect
    {
        $session->flashErrors($errors);
        $session->flashInput(array_intersect_key($form->fields, array_flip($keep)));
        return Redirect::to($to);
    }
}
