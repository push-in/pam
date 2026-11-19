<?php

declare(strict_types=1);

namespace App\Controllers;

use Nimbus\Auth\PasswordResets;
use Nimbus\Http\Redirect;
use Nimbus\Session\Session;
use Nimbus\View\View;
use Pam\Http\Request;

final readonly class PasswordResetController
{
    public function __construct(private PasswordResets $passwords)
    {
    }

    public function request(): View
    {
        return view('auth.forgot-password');
    }

    public function email(Request $request, Session $session): Redirect
    {
        $form = Form::from($request);
        $email = $form->string('email');
        if (filter_var($email, FILTER_VALIDATE_EMAIL) === false) {
            return Form::back($session, '/forgot-password', ['email' => [__('validation.email')]], $form, ['email']);
        }
        // The same answer for every address, so the form does not reveal who has an account.
        $this->passwords->sendResetLink(strtolower($email));
        $session->flash('status', __('passwords.sent'));
        return Redirect::to('/forgot-password');
    }

    public function edit(Request $request): View
    {
        return view('auth.reset-password', ['token' => (string) $request->route('token')]);
    }

    public function update(Request $request, Session $session): Redirect
    {
        $form = Form::from($request);
        $token = $form->string('token');
        $password = $form->secret('password');
        $back = '/reset-password/' . rawurlencode($token);
        if ($password !== $form->secret('password_confirmation')) {
            return Form::back($session, $back, ['password' => [__('validation.confirmed')]], $form);
        }
        try {
            $reset = $this->passwords->reset($token, $password);
        } catch (\InvalidArgumentException) {
            return Form::back($session, $back, ['password' => [__('validation.password', ['min' => 8])]], $form);
        }
        if (!$reset) {
            return Form::back($session, '/forgot-password', ['email' => [__('passwords.token')]], $form);
        }
        $session->flash('status', __('passwords.reset'));
        return Redirect::to('/login');
    }
}
