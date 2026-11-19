<?php

declare(strict_types=1);

namespace App\Controllers;

use App\Auth\Users;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Http\Redirect;
use Nimbus\Session\Session;
use Nimbus\View\View;
use Pam\Http\Request;

final readonly class RegisterController
{
    public function __construct(
        private Users $users,
        private SessionAuthenticator $sessions,
    ) {
    }

    public function show(): View
    {
        return view('auth.register');
    }

    public function store(Request $request, Session $session): Redirect
    {
        $form = Form::from($request);
        $name = $form->string('name');
        $email = Users::normalize($form->string('email'));
        $password = $form->secret('password');

        $errors = [];
        if ($name === '' || mb_strlen($name) > 120) {
            $errors['name'][] = __('validation.name');
        }
        if (filter_var($email, FILTER_VALIDATE_EMAIL) === false || strlen($email) > 254) {
            $errors['email'][] = __('validation.email');
        } elseif ($this->users->findByEmail($email) !== null) {
            $errors['email'][] = __('validation.email_taken');
        }
        if (mb_strlen($password) < 8 || strlen($password) > 1_024) {
            $errors['password'][] = __('validation.password', ['min' => 8]);
        } elseif ($password !== $form->secret('password_confirmation')) {
            $errors['password'][] = __('validation.confirmed');
        }
        if ($errors !== []) {
            return Form::back($session, '/register', $errors, $form, ['name', 'email']);
        }

        $this->sessions->login($this->users->create($name, $email, $password));
        return Redirect::to('/dashboard');
    }
}
