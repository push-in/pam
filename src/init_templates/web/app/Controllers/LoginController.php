<?php

declare(strict_types=1);

namespace App\Controllers;

use App\Auth\Users;
use Nimbus\Auth\SessionAuthenticator;
use Nimbus\Http\Redirect;
use Nimbus\Session\Session;
use Nimbus\View\View;
use Pam\Http\Request;

final readonly class LoginController
{
    public function __construct(
        private Users $users,
        private SessionAuthenticator $sessions,
    ) {
    }

    public function show(Request $request): View|Redirect
    {
        return $this->sessions->authenticate($request) === null
            ? view('auth.login')
            : Redirect::to('/dashboard');
    }

    public function store(Request $request, Session $session): Redirect
    {
        $form = Form::from($request);
        $user = $this->users->attempt($form->string('email'), $form->secret('password'));
        if ($user === null) {
            return Form::back($session, '/login', ['email' => [__('auth.failed')]], $form, ['email']);
        }
        $this->sessions->login($user);
        return Redirect::to('/dashboard');
    }

    public function destroy(Session $session): Redirect
    {
        $this->sessions->logout();
        $session->flash('status', __('auth.logged_out'));
        return Redirect::to('/login');
    }
}
